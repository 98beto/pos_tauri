//! Pure, bounded CFDI extraction. No filesystem, database, networking or fiscal validation.
use crate::{error::AppError, models::purchase_invoice::*};
use roxmltree::{Document, Node, ParsingOptions};

pub const MAX_XML_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_XML_NODES: u32 = 200_000;
pub const MAX_XML_DEPTH: usize = 64;
pub const MAX_CONCEPTS: usize = 10_000;
pub const MAX_VALUE_BYTES: usize = 16_384;
const CFDI: &str = "http://www.sat.gob.mx/cfd/4";
const TIMBRE: &str = "http://www.sat.gob.mx/TimbreFiscalDigital";
type Result<T> = std::result::Result<T, AppError>;

fn error(code: &'static str) -> AppError {
    AppError::validation(
        code,
        match code {
            "INVOICE_XML_LIMIT" => "El XML excede los límites de recursos permitidos",
            "INVOICE_XML_DTD" => "No se permiten declaraciones DTD",
            "INVOICE_UNSUPPORTED_VERSION" => "Sólo se admite CFDI 4.0",
            "INVOICE_UNSUPPORTED_TYPE" => "Sólo se admiten comprobantes de ingreso",
            "INVOICE_INVALID_UUID" => "Se requiere un único timbre con UUID válido",
            "INVOICE_REQUIRED_FIELD" => "Falta un dato requerido del comprobante",
            "INVOICE_AMBIGUOUS_XML" => "El comprobante contiene elementos ambiguos",
            "INVOICE_INVALID_DECIMAL" => "Decimal inválido: máximo 18 enteros y 6 decimales",
            "INVOICE_INVALID_DATE" => "Fecha u hora del comprobante inválida",
            "INVOICE_INVALID_CURRENCY" => "La moneda debe contener tres letras mayúsculas",
            "INVOICE_INVALID_RFC" => "RFC inválido",
            _ => "XML inválido o codificación distinta de UTF-8",
        },
    )
}

fn optional(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.attribute(name).map(str::to_owned)
}
fn required(node: Node<'_, '_>, name: &str) -> Result<String> {
    optional(node, name)
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| error("INVOICE_REQUIRED_FIELD"))
}
fn one<'a, 'input>(
    node: Node<'a, 'input>,
    ns: &str,
    name: &str,
    mandatory: bool,
) -> Result<Option<Node<'a, 'input>>> {
    let mut matches = node.children().filter(|n| n.has_tag_name((ns, name)));
    let first = matches.next();
    if matches.next().is_some() {
        return Err(error("INVOICE_AMBIGUOUS_XML"));
    }
    if mandatory && first.is_none() {
        return Err(error("INVOICE_REQUIRED_FIELD"));
    }
    Ok(first)
}
fn cfdi_one<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Result<Node<'a, 'input>> {
    one(node, CFDI, name, true)?.ok_or_else(|| error("INVOICE_REQUIRED_FIELD"))
}

fn decimal(value: String, positive: bool) -> Result<String> {
    let (whole, fraction) = value
        .split_once('.')
        .map_or((value.as_str(), None), |(w, f)| (w, Some(f)));
    if whole.is_empty()
        || whole.len() > 18
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction
            .is_some_and(|f| f.is_empty() || f.len() > 6 || !f.bytes().all(|b| b.is_ascii_digit()))
        || (positive && !value.bytes().any(|b| matches!(b, b'1'..=b'9')))
    {
        return Err(error("INVOICE_INVALID_DECIMAL"));
    }
    Ok(value)
}
fn dec(node: Node<'_, '_>, name: &str, positive: bool) -> Result<String> {
    decimal(required(node, name)?, positive)
}
fn opt_dec(node: Node<'_, '_>, name: &str) -> Result<Option<String>> {
    optional(node, name).map(|v| decimal(v, false)).transpose()
}

fn datetime(value: String) -> Result<String> {
    let b = value.as_bytes();
    if b.len() != 19
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b.iter()
            .enumerate()
            .any(|(i, c)| !matches!(i, 4 | 7 | 10 | 13 | 16) && !c.is_ascii_digit())
    {
        return Err(error("INVOICE_INVALID_DATE"));
    }
    let num = |start: usize, end: usize| -> u32 {
        b[start..end]
            .iter()
            .fold(0, |v, c| v * 10 + u32::from(c - b'0'))
    };
    let y = num(0, 4);
    let m = num(5, 7);
    let d = num(8, 10);
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if y == 0 || d == 0 || d > days || num(11, 13) > 23 || num(14, 16) > 59 || num(17, 19) > 59 {
        return Err(error("INVOICE_INVALID_DATE"));
    }
    Ok(value)
}
fn party(node: Node<'_, '_>) -> Result<InvoiceParty> {
    let rfc = required(node, "Rfc")?.trim().to_uppercase();
    let chars: Vec<char> = rfc.chars().collect();
    let prefix = chars.len().saturating_sub(9);
    if !matches!(chars.len(), 12 | 13)
        || !chars[..prefix]
            .iter()
            .all(|c| c.is_ascii_uppercase() || matches!(c, 'Ñ' | '&'))
        || !chars[prefix..prefix + 6].iter().all(char::is_ascii_digit)
        || !chars[prefix + 6..].iter().all(char::is_ascii_alphanumeric)
    {
        return Err(error("INVOICE_INVALID_RFC"));
    }
    Ok(InvoiceParty {
        rfc,
        name: required(node, "Nombre")?,
    })
}

// Validate the fiscal skeleton before extraction, without interpreting extensions.
fn validate_fiscal_structure(root: Node<'_, '_>) -> Result<()> {
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        for child in node.children().filter(Node::is_element) {
            let name = child.tag_name().name();
            let parent = node.tag_name().name();
            if node == root
                && (child.has_tag_name((CFDI, "Addenda"))
                    || child.has_tag_name((CFDI, "Complemento")))
                || node.has_tag_name((CFDI, "Concepto"))
                    && child.has_tag_name((CFDI, "ComplementoConcepto"))
            {
                continue;
            }
            let expected_parent = match name {
                "Conceptos" => Some(&["Comprobante"][..]),
                "Concepto" => Some(&["Conceptos"][..]),
                "Impuestos" => Some(&["Comprobante", "Concepto"][..]),
                "Traslados" | "Retenciones" => Some(&["Impuestos"][..]),
                "Traslado" => Some(&["Traslados"][..]),
                "Retencion" => Some(&["Retenciones"][..]),
                _ => None,
            };
            if expected_parent.is_some_and(|parents| {
                child.tag_name().namespace() != Some(CFDI)
                    || node.tag_name().namespace() != Some(CFDI)
                    || !parents.contains(&parent)
                    || (parent == "Comprobante" && node != root)
            }) || match parent {
                "Conceptos" => !child.has_tag_name((CFDI, "Concepto")),
                "Impuestos" => {
                    !child.has_tag_name((CFDI, "Traslados"))
                        && !child.has_tag_name((CFDI, "Retenciones"))
                }
                "Traslados" => !child.has_tag_name((CFDI, "Traslado")),
                "Retenciones" => !child.has_tag_name((CFDI, "Retencion")),
                _ => false,
            } {
                return Err(error("INVOICE_AMBIGUOUS_XML"));
            }
            pending.push(child);
        }
    }
    Ok(())
}

fn taxes(parent: Node<'_, '_>) -> Result<(Vec<InvoiceTax>, Option<String>, Option<String>)> {
    let Some(node) = one(parent, CFDI, "Impuestos", false)? else {
        return Ok((vec![], None, None));
    };
    let mut taxes = vec![];
    for (container, item, kind) in [
        ("Traslados", "Traslado", InvoiceTaxKind::Transferred),
        ("Retenciones", "Retencion", InvoiceTaxKind::Withheld),
    ] {
        if let Some(group) = one(node, CFDI, container, false)? {
            for tax in group.children().filter(|n| n.has_tag_name((CFDI, item))) {
                taxes.push(InvoiceTax {
                    kind,
                    code: required(tax, "Impuesto")?,
                    base: opt_dec(tax, "Base")?,
                    factor_type: optional(tax, "TipoFactor"),
                    rate: opt_dec(tax, "TasaOCuota")?,
                    amount: opt_dec(tax, "Importe")?,
                });
            }
        }
    }
    Ok((
        taxes,
        opt_dec(node, "TotalImpuestosTrasladados")?,
        opt_dec(node, "TotalImpuestosRetenidos")?,
    ))
}

/// Extracts source decimals and local timestamps without calculation or timezone conversion.
pub fn parse_purchase_invoice(bytes: &[u8]) -> Result<InvoiceDocument> {
    if bytes.len() > MAX_XML_BYTES {
        return Err(error("INVOICE_XML_LIMIT"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| error("INVOICE_INVALID_XML"))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // roxmltree permits empty DTDs even with allow_dtd=false. Reject those too.
    if has_doctype(text) {
        return Err(error("INVOICE_XML_DTD"));
    }
    let doc = Document::parse_with_options(
        text,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_XML_NODES,
            entity_resolver: None,
        },
    )
    .map_err(|e| {
        error(match e {
            roxmltree::Error::NodesLimitReached => "INVOICE_XML_LIMIT",
            roxmltree::Error::DtdDetected => "INVOICE_XML_DTD",
            _ => "INVOICE_INVALID_XML",
        })
    })?;
    // Iterative traversal, including unknown addenda. Never recurse over untrusted XML.
    let mut pending = vec![(doc.root(), 0)];
    while let Some((n, depth)) = pending.pop() {
        if depth > MAX_XML_DEPTH
            || n.attributes().len() > 128
            || n.attributes()
                .any(|a| a.value().len() > MAX_VALUE_BYTES || a.name().len() > 256)
            || n.text().is_some_and(|t| t.len() > MAX_VALUE_BYTES)
            || (n.is_element() && n.tag_name().name().len() > 256)
            || n.namespaces().len() > 128
            || n.namespaces().any(|ns| {
                ns.uri().len() > MAX_VALUE_BYTES || ns.name().is_some_and(|name| name.len() > 256)
            })
        {
            return Err(error("INVOICE_XML_LIMIT"));
        }
        pending.extend(
            n.children()
                .map(|child| (child, depth + usize::from(child.is_element()))),
        );
    }
    let root = doc.root_element();
    if !root.has_tag_name((CFDI, "Comprobante")) || root.attribute("Version") != Some("4.0") {
        return Err(error("INVOICE_UNSUPPORTED_VERSION"));
    }
    if root.attribute("TipoDeComprobante") != Some("I") {
        return Err(error("INVOICE_UNSUPPORTED_TYPE"));
    }
    validate_fiscal_structure(root)?;
    let complement = cfdi_one(root, "Complemento")?;
    let mut stamps = root
        .descendants()
        .filter(|n| n.has_tag_name((TIMBRE, "TimbreFiscalDigital")));
    let stamp = stamps.next().ok_or_else(|| error("INVOICE_INVALID_UUID"))?;
    if stamps.next().is_some() || stamp.parent() != Some(complement) {
        return Err(error("INVOICE_INVALID_UUID"));
    }
    let uuid = stamp.attribute("UUID").unwrap_or("").to_uppercase();
    if uuid.len() != 36
        || uuid.bytes().enumerate().any(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b != b'-'
            } else {
                !b.is_ascii_hexdigit()
            }
        })
    {
        return Err(error("INVOICE_INVALID_UUID"));
    }
    let currency = required(root, "Moneda")?;
    if currency.len() != 3 || !currency.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(error("INVOICE_INVALID_CURRENCY"));
    }
    let issued_at = datetime(required(root, "Fecha")?)?;
    let concept_group = cfdi_one(root, "Conceptos")?;
    let mut concepts = vec![];
    for node in concept_group
        .children()
        .filter(|n| n.has_tag_name((CFDI, "Concepto")))
    {
        if concepts.len() == MAX_CONCEPTS {
            return Err(error("INVOICE_XML_LIMIT"));
        }
        concepts.push(InvoiceConcept {
            supplier_code: optional(node, "NoIdentificacion"),
            product_service_code: required(node, "ClaveProdServ")?,
            unit_code: required(node, "ClaveUnidad")?,
            unit: optional(node, "Unidad"),
            description: required(node, "Descripcion")?,
            quantity: dec(node, "Cantidad", true)?,
            unit_price: dec(node, "ValorUnitario", false)?,
            amount: dec(node, "Importe", false)?,
            discount: opt_dec(node, "Descuento")?,
            taxes: taxes(node)?.0,
        });
    }
    if concepts.is_empty() {
        return Err(error("INVOICE_REQUIRED_FIELD"));
    }
    let (taxes, total_transferred_taxes, total_withheld_taxes) = taxes(root)?;
    Ok(InvoiceDocument {
        version: "4.0".into(),
        document_type: "I".into(),
        issuer: party(cfdi_one(root, "Emisor")?)?,
        receiver: party(cfdi_one(root, "Receptor")?)?,
        uuid,
        series: optional(root, "Serie"),
        folio: optional(root, "Folio"),
        issued_date: issued_at[..10].into(),
        issued_at,
        stamped_at: optional(stamp, "FechaTimbrado").map(datetime).transpose()?,
        currency,
        exchange_rate: opt_dec(root, "TipoCambio")?,
        subtotal: dec(root, "SubTotal", false)?,
        discount: opt_dec(root, "Descuento")?,
        total: dec(root, "Total", false)?,
        total_transferred_taxes,
        total_withheld_taxes,
        taxes,
        concepts,
    })
}

// Skip comments, CDATA and processing instructions, where markup is inert.
fn has_doctype(mut text: &str) -> bool {
    while let Some(start) = text.find('<') {
        text = &text[start..];
        let end = if text.starts_with("<!--") {
            Some("-->")
        } else if text.starts_with("<![CDATA[") {
            Some("]]>")
        } else if text.starts_with("<?") {
            Some("?>")
        } else {
            None
        };
        if let Some(end) = end {
            let Some(index) = text.find(end) else {
                return false;
            };
            text = &text[index + end.len()..];
        } else {
            if text.starts_with("<!DOCTYPE") {
                return true;
            }
            text = &text[1..];
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    const MINIMAL: &str = include_str!("../../tests/fixtures/cfdi/minimal.xml");
    const INCOME: &str = include_str!("../../tests/fixtures/cfdi/income.xml");
    fn parse(s: &str) -> Result<InvoiceDocument> {
        parse_purchase_invoice(s.as_bytes())
    }
    fn fails(s: &str, code: &str) {
        assert_eq!(parse(s).unwrap_err().code, code);
    }

    #[test]
    fn purchase_invoice_extracts_original_precision_order_and_taxes() {
        let d = parse(INCOME).unwrap();
        assert_eq!(d.issuer.rfc, "AAA010101AAA");
        assert_eq!(d.issuer.name, "Café y Té S.A.");
        assert_eq!(d.uuid, "01234567-89AB-CDEF-0123-456789ABCDEF");
        assert_eq!(d.issued_date, "2024-02-29");
        assert_eq!(d.issued_at, "2024-02-29T23:59:59");
        assert_eq!(d.currency, "USD");
        assert_eq!(d.subtotal, "0010.123456");
        assert_eq!(d.exchange_rate.as_deref(), Some("17.000001"));
        assert_eq!(d.concepts[0].quantity, "0.000001");
        assert_eq!(d.concepts[0].supplier_code.as_deref(), Some("000ABC"));
        assert_eq!(d.concepts[0].description, "Café de origen & té");
        assert_eq!(d.concepts[0].unit.as_deref(), Some("Kilógramo"));
        assert_eq!(d.concepts[0].discount.as_deref(), Some("0"));
        assert_eq!(d.concepts[0].taxes[1].kind, InvoiceTaxKind::Withheld);
        assert_eq!(d.concepts[1].quantity, "2.000");
        assert_eq!(d.concepts[1].discount, None);
        assert_eq!(
            d.concepts[1].taxes[0].factor_type.as_deref(),
            Some("Exento")
        );
        assert_eq!(d.concepts[1].taxes[0].amount, None);
        assert_eq!(d.taxes[1].base, None);
        assert_eq!(d.total_withheld_taxes.as_deref(), Some("0.1"));
        // Deliberately inconsistent arithmetic is extracted, not corrected or rejected.
        assert_eq!(d.total, "11.743209");
    }
    #[test]
    fn purchase_invoice_optional_nulls_default_namespace_bom_and_serde_contract() {
        let d = parse(&format!("\u{feff}{MINIMAL}")).unwrap();
        assert_eq!(d.series, None);
        assert_eq!(d.folio, None);
        assert_eq!(d.stamped_at, None);
        assert_eq!(d.discount, None);
        assert_eq!(d.total_transferred_taxes, None);
        assert!(d.taxes.is_empty());
        let value = serde_json::to_value(&d).unwrap();
        assert!(value["exchange_rate"].is_null());
        assert_eq!(value["concepts"][0]["quantity"], "1");
        assert!(value.get("issuedAt").is_none());
        assert_eq!(serde_json::from_value::<InvoiceDocument>(value).unwrap(), d);
        assert_eq!(
            serde_json::to_value(InvoiceRegistrationOutcome::Existing).unwrap(),
            "existing"
        );
        assert_eq!(
            serde_json::to_value(InvoiceAttachmentKind::Xml).unwrap(),
            "xml"
        );
        assert_eq!(PURCHASE_INVOICE_PAGE_SIZE, 25);
    }
    #[test]
    fn purchase_invoice_namespace_and_parent_scoping() {
        assert_eq!(
            parse(
                &INCOME
                    .replace("c:", "invoice:")
                    .replace("xmlns:c=", "xmlns:invoice=")
                    .replace("t:", "stamp:")
                    .replace("xmlns:t=", "xmlns:stamp=")
            )
            .unwrap(),
            parse(INCOME).unwrap()
        );
        fails(
            &MINIMAL.replace("http://www.sat.gob.mx/cfd/4", "urn:wrong"),
            "INVOICE_UNSUPPORTED_VERSION",
        );
        fails(
            &MINIMAL.replace("Version=\"4.0\"", "Version=\"3.3\""),
            "INVOICE_UNSUPPORTED_VERSION",
        );
        for kind in ["E", "P", "T", "N"] {
            fails(
                &MINIMAL.replace(
                    "TipoDeComprobante=\"I\"",
                    &format!("TipoDeComprobante=\"{kind}\""),
                ),
                "INVOICE_UNSUPPORTED_TYPE",
            );
        }
        let fake = "<Addenda><Emisor Rfc=\"BBB010101BBB\" Nombre=\"Falso\"/><Conceptos><Concepto/></Conceptos><Impuestos TotalImpuestosTrasladados=\"999\"/></Addenda>";
        let xml = MINIMAL.replace("</Comprobante>", &format!("{fake}</Comprobante>"));
        let d = parse(&xml).unwrap();
        assert_eq!(d.issuer.rfc, "AAA010101AAA");
        assert_eq!(d.concepts.len(), 1);
        assert_eq!(d.total_transferred_taxes, None);
        fails(
            &xml.replace(
                "<Emisor Rfc=\"AAA010101AAA\" Nombre=\"Emisor sintético\"/>",
                "",
            ),
            "INVOICE_REQUIRED_FIELD",
        );
        fails(
            &MINIMAL.replace("<Conceptos>", "<Conceptos/><Conceptos>"),
            "INVOICE_AMBIGUOUS_XML",
        );
        fails(
            &MINIMAL.replace(
                "<Receptor",
                "<Emisor Rfc=\"BBB010101BBB\" Nombre=\"Otro\"/><Receptor",
            ),
            "INVOICE_AMBIGUOUS_XML",
        );
    }
    #[test]
    fn purchase_invoice_rejects_mixed_misplaced_fiscal_elements() {
        let bad = "INVOICE_AMBIGUOUS_XML";
        for extra in [
            "<Concepto xmlns=\"urn:wrong\"/>",
            "<Wrapper><Concepto/></Wrapper>",
            "<Wrapper/>",
        ] {
            fails(
                &MINIMAL.replace("</Conceptos>", &format!("{extra}</Conceptos>")),
                bad,
            );
        }
        for extra in [
            "<Impuestos xmlns=\"urn:wrong\" TotalImpuestosTrasladados=\"99\"/>",
            "<Wrapper><Impuestos/></Wrapper>",
            "<c:Comprobante><c:Impuestos/></c:Comprobante>",
        ] {
            // Both the header and a line already have valid taxes in INCOME.
            fails(
                &INCOME.replace("</c:Comprobante>", &format!("{extra}</c:Comprobante>")),
                bad,
            );
            fails(
                &INCOME.replacen("</c:Concepto>", &format!("{extra}</c:Concepto>"), 1),
                bad,
            );
        }
        for (closing, extras) in [
            (
                "</c:Impuestos>",
                vec![
                    "<Traslados xmlns=\"urn:wrong\"/>",
                    "<Retenciones xmlns=\"urn:wrong\"/>",
                    "<Wrapper><c:Traslados/></Wrapper>",
                    "<Wrapper><c:Retenciones/></Wrapper>",
                ],
            ),
            (
                "</c:Traslados>",
                vec![
                    "<Traslado xmlns=\"urn:wrong\"/>",
                    "<Wrapper><c:Traslado/></Wrapper>",
                    "<c:Retencion/>",
                ],
            ),
            (
                "</c:Retenciones>",
                vec![
                    "<Retencion xmlns=\"urn:wrong\"/>",
                    "<Wrapper><c:Retencion/></Wrapper>",
                    "<c:Traslado/>",
                ],
            ),
        ] {
            for extra in extras {
                // Exercise both line and header containers independently.
                let first = INCOME.find(closing).unwrap();
                let last = INCOME.rfind(closing).unwrap();
                for index in [first, last] {
                    let mut xml = INCOME.to_owned();
                    xml.insert_str(index, extra);
                    fails(&xml, bad);
                }
            }
        }
        // Extension payloads remain opaque even when names resemble fiscal nodes.
        let xml = MINIMAL
            .replace(
                "</Complemento>",
                "<ext:Other xmlns:ext=\"urn:extension\"><ext:Impuestos/></ext:Other></Complemento>",
            )
            .replace(
                "</Comprobante>",
                "<Addenda><Concepto/><Impuestos/></Addenda></Comprobante>",
            );
        assert!(parse(&xml).is_ok());
        let xml = INCOME.replacen(
            "</c:Concepto>",
            "<c:ComplementoConcepto><ext:Custom xmlns:ext=\"urn:extension\"><ext:Impuestos/></ext:Custom></c:ComplementoConcepto></c:Concepto>",
            1,
        );
        assert!(parse(&xml).is_ok());
    }

    #[test]
    fn purchase_invoice_limits_comments_before_and_after_document_element() {
        for size in [MAX_VALUE_BYTES, MAX_VALUE_BYTES + 1] {
            let comment = format!("<!--{}-->", "x".repeat(size));
            for xml in [format!("{comment}{MINIMAL}"), format!("{MINIMAL}{comment}")] {
                if size == MAX_VALUE_BYTES {
                    assert!(parse(&xml).is_ok());
                } else {
                    fails(&xml, "INVOICE_XML_LIMIT");
                }
            }
        }
    }
    #[test]
    fn purchase_invoice_requires_single_well_formed_scoped_uuid() {
        for uuid in [
            "",
            "123",
            "01234567_89AB-CDEF-0123-456789ABCDEF",
            "G1234567-89AB-CDEF-0123-456789ABCDEF",
        ] {
            fails(
                &MINIMAL.replace("01234567-89AB-CDEF-0123-456789ABCDEF", uuid),
                "INVOICE_INVALID_UUID",
            );
        }
        fails(&MINIMAL.replace("UUID=", "Absent="), "INVOICE_INVALID_UUID");
        fails(&MINIMAL.replace("</Complemento>", "<TimbreFiscalDigital xmlns=\"http://www.sat.gob.mx/TimbreFiscalDigital\"/></Complemento>"), "INVOICE_INVALID_UUID");
        fails(
            &MINIMAL
                .replace("<TimbreFiscalDigital", "<Wrapper><TimbreFiscalDigital")
                .replace("/></Complemento>", "/></Wrapper></Complemento>"),
            "INVOICE_INVALID_UUID",
        );
        fails(
            &MINIMAL.replace("http://www.sat.gob.mx/TimbreFiscalDigital", "urn:wrong"),
            "INVOICE_INVALID_UUID",
        );
    }
    #[test]
    fn purchase_invoice_decimal_calendar_currency_and_rfc_bounds() {
        for bad in [
            "-1",
            "+1",
            "1e2",
            ".1",
            "1.",
            "1.1234567",
            "1234567890123456789",
            " 1",
            "NaN",
            "",
        ] {
            fails(
                &MINIMAL.replace("Total=\"0\"", &format!("Total=\"{bad}\"")),
                if bad.is_empty() {
                    "INVOICE_REQUIRED_FIELD"
                } else {
                    "INVOICE_INVALID_DECIMAL"
                },
            );
        }
        for zero in ["0", "00.000000"] {
            fails(
                &MINIMAL.replace("Cantidad=\"1\"", &format!("Cantidad=\"{zero}\"")),
                "INVOICE_INVALID_DECIMAL",
            );
        }
        assert!(parse(
            &MINIMAL.replace("Cantidad=\"1\"", "Cantidad=\"999999999999999999.999999\"")
        )
        .is_ok());
        for date in [
            "2025-02-29T00:00:00",
            "1900-02-29T00:00:00",
            "2024-04-31T00:00:00",
            "0000-01-01T00:00:00",
            "2024-00-01T00:00:00",
            "2024-01-00T00:00:00",
            "2024-01-01T24:00:00",
            "2024-01-01T00:60:00",
            "2024-01-01T00:00:60",
            "2024-01-01T00:00:00Z",
            "é",
        ] {
            fails(
                &MINIMAL.replace("2026-11-06T00:00:00", date),
                "INVOICE_INVALID_DATE",
            );
        }
        assert!(parse(&MINIMAL.replace("2026-11-06", "2000-02-29")).is_ok());
        for currency in ["mxn", "US", "ABCD", "💰"] {
            fails(
                &MINIMAL.replace("MXN", currency),
                "INVOICE_INVALID_CURRENCY",
            );
        }
        for rfc in ["A", "AAAxxxxxxAAA", "AAA010101A-A"] {
            fails(&MINIMAL.replace("AAA010101AAA", rfc), "INVOICE_INVALID_RFC");
        }
        fails(
            &INCOME.replace("2024-03-01T00:00:01", "2024-02-30T00:00:01"),
            "INVOICE_INVALID_DATE",
        );
    }
    #[test]
    fn purchase_invoice_rejects_malformed_entities_and_dtd_even_empty() {
        fails("<broken", "INVOICE_INVALID_XML");
        fails(
            &MINIMAL.replace("Muestra", "&external;"),
            "INVOICE_INVALID_XML",
        );
        for dtd in [
            "<!DOCTYPE Comprobante>",
            "<!DOCTYPE Comprobante [<!ENTITY x SYSTEM 'file:///etc/passwd'>]>",
            "<!DOCTYPE Comprobante [<!ENTITY x 'abc'>]>",
        ] {
            fails(&format!("{dtd}{MINIMAL}"), "INVOICE_XML_DTD");
        }
        assert_eq!(
            parse_purchase_invoice(&[0xff, 0xfe]).unwrap_err().code,
            "INVOICE_INVALID_XML"
        );
        assert!(parse(&MINIMAL.replace("</Comprobante>", "<Addenda><![CDATA[<!DOCTYPE inert>]]><!-- <!DOCTYPE inert> --></Addenda></Comprobante>")).is_ok());
    }
    #[test]
    fn purchase_invoice_resource_limits_and_ten_thousand_taxed_concepts() {
        assert_eq!(
            parse_purchase_invoice(&vec![b' '; MAX_XML_BYTES + 1])
                .unwrap_err()
                .code,
            "INVOICE_XML_LIMIT"
        );
        let addenda = |inside: String| {
            MINIMAL.replace(
                "</Comprobante>",
                &format!("<Addenda>{inside}</Addenda></Comprobante>"),
            )
        };
        assert!(parse(&addenda(format!(
            "{}{}",
            "<x>".repeat(62),
            "</x>".repeat(62)
        )))
        .is_ok());
        fails(
            &addenda(format!("{}{}", "<x>".repeat(63), "</x>".repeat(63))),
            "INVOICE_XML_LIMIT",
        );
        fails(
            &addenda("<x/>".repeat(MAX_XML_NODES as usize)),
            "INVOICE_XML_LIMIT",
        );
        fails(
            &MINIMAL.replace("Muestra", &"x".repeat(MAX_VALUE_BYTES + 1)),
            "INVOICE_XML_LIMIT",
        );
        fails(
            &addenda("x".repeat(MAX_VALUE_BYTES + 1)),
            "INVOICE_XML_LIMIT",
        );
        let attributes = (0..129).map(|i| format!(" a{i}=\"x\"")).collect::<String>();
        fails(&addenda(format!("<x{attributes}/>")), "INVOICE_XML_LIMIT");
        fails(
            &addenda(format!("<{} />", "x".repeat(257))),
            "INVOICE_XML_LIMIT",
        );
        fails(
            &addenda(format!("<x {}=\"y\"/>", "a".repeat(257))),
            "INVOICE_XML_LIMIT",
        );
        fails(
            &addenda(format!(
                "<x xmlns:q=\"{}\"/>",
                "a".repeat(MAX_VALUE_BYTES + 1)
            )),
            "INVOICE_XML_LIMIT",
        );
        // Use compact form to stay below 5 MiB while still exceeding a 10k node limit.
        let line = "<Concepto ClaveProdServ=\"1\" ClaveUnidad=\"H87\" Descripcion=\"x\" Cantidad=\"1\" ValorUnitario=\"0\" Importe=\"0\"><Impuestos><Traslados><Traslado Impuesto=\"002\" TipoFactor=\"Exento\" Base=\"0\"/></Traslados></Impuestos></Concepto>";
        let original = "<Concepto ClaveProdServ=\"01010101\" ClaveUnidad=\"H87\" Descripcion=\"Muestra\" Cantidad=\"1\" ValorUnitario=\"0\" Importe=\"0\"/>";
        let many = MINIMAL.replace(original, &line.repeat(MAX_CONCEPTS));
        assert!(many.len() < MAX_XML_BYTES);
        assert_eq!(parse(&many).unwrap().concepts.len(), MAX_CONCEPTS);
        fails(
            &MINIMAL.replace(original, &line.repeat(MAX_CONCEPTS + 1)),
            "INVOICE_XML_LIMIT",
        );
    }
}
