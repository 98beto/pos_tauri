use super::{normalize_search, purchase_invoice_files::CapturedFile, search_key, validate_id};
use crate::{
    error::AppError,
    models::{purchase_invoice::*, user::User},
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
#[cfg(test)]
#[path = "purchase_invoices_tests.rs"]
mod tests;
type Result<T> = std::result::Result<T, AppError>;

pub fn kind(k: InvoiceAttachmentKind) -> &'static str {
    match k {
        InvoiceAttachmentKind::Xml => "xml",
        InvoiceAttachmentKind::Pdf => "pdf",
    }
}
fn json<T: serde::de::DeserializeOwned>(value: String) -> Result<T> {
    serde_json::from_str(&value).map_err(|_| AppError::database())
}
fn encode<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|_| AppError::database())
}
pub fn attachment_metadata(
    c: &Connection,
    id: i64,
    k: InvoiceAttachmentKind,
) -> Result<Option<InvoiceAttachmentMetadata>> {
    Ok(c.query_row("SELECT original_name,mime_type,size_bytes,sha256 FROM purchase_invoice_attachments WHERE invoice_id=?1 AND kind=?2", params![id,kind(k)], |r| Ok(InvoiceAttachmentMetadata { kind:k, original_name:r.get(0)?, mime_type:r.get(1)?, size_bytes:r.get(2)?, sha256:r.get(3)? })).optional()?)
}
pub fn detail(c: &Connection, id: i64) -> Result<PurchaseInvoiceDetail> {
    validate_id(id)?;
    let (header,audit) = c.query_row("SELECT header_json,registered_at,registered_by_user_id,registered_by_name FROM purchase_invoices WHERE id=?1",[id], |r|Ok((r.get::<_,String>(0)?,PurchaseInvoiceAudit {registered_at:r.get(1)?,registered_by_user_id:r.get(2)?,registered_by_name:r.get(3)?}))).optional()?.ok_or_else(||AppError::not_found("INVOICE_NOT_FOUND","Factura no encontrada"))?;
    let mut document: InvoiceDocument = json(header)?;
    let lines = c
        .prepare(
            "SELECT concept_json FROM purchase_invoice_lines WHERE invoice_id=?1 ORDER BY position",
        )?
        .query_map([id], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    document.concepts = lines.into_iter().map(json).collect::<Result<_>>()?;
    Ok(PurchaseInvoiceDetail {
        id,
        document,
        audit,
        xml: attachment_metadata(c, id, InvoiceAttachmentKind::Xml)?
            .ok_or_else(AppError::database)?,
        pdf: attachment_metadata(c, id, InvoiceAttachmentKind::Pdf)?,
    })
}
fn insert_attachment(c: &Connection, id: i64, file: &CapturedFile) -> Result<()> {
    c.execute("INSERT INTO purchase_invoice_attachments(invoice_id,kind,original_name,mime_type,size_bytes,sha256,content) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,kind(file.metadata.kind),file.metadata.original_name,file.metadata.mime_type,file.metadata.size_bytes,file.metadata.sha256,&*file.bytes])?;
    Ok(())
}
/// Caller reparses immutable bytes before acquiring session/SQL locks.
pub fn register(
    c: &mut Connection,
    user: &User,
    document: &InvoiceDocument,
    xml: &CapturedFile,
    pdf: Option<&CapturedFile>,
) -> Result<InvoiceRegistrationResult> {
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some((id, hash)) = tx
        .query_row(
            "SELECT id,xml_sha256 FROM purchase_invoices WHERE uuid=?1",
            [&document.uuid],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if hash != xml.metadata.sha256 {
            return Err(AppError::conflict(
                "INVOICE_CONTENT_DIFFERENT",
                "El UUID ya tiene otro XML",
            ));
        }
        return Ok(InvoiceRegistrationResult {
            outcome: InvoiceRegistrationOutcome::Existing,
            detail: detail(&tx, id)?,
        });
    }
    let mut header = document.clone();
    header.concepts.clear();
    let number = match (
        document.series.as_deref().filter(|s| !s.is_empty()),
        document.folio.as_deref().filter(|s| !s.is_empty()),
    ) {
        (Some(series), Some(folio)) => format!("{series}-{folio}"),
        (Some(part), None) | (None, Some(part)) => part.to_owned(),
        (None, None) => document.uuid.clone(),
    };
    let number = search_key(&number);
    tx.execute("INSERT INTO purchase_invoices(uuid,xml_sha256,issuer_rfc,issuer_name,issuer_search_key,receiver_rfc,receiver_name,series,folio,number_search_key,issued_at,issued_date,currency,total,header_json,registered_by_user_id,registered_by_name) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",params![document.uuid,xml.metadata.sha256,document.issuer.rfc,document.issuer.name,search_key(&document.issuer.name),document.receiver.rfc,document.receiver.name,document.series,document.folio,number,document.issued_at,document.issued_date,document.currency,document.total,encode(&header)?,user.id,format!("{} {}",user.first_name,user.last_name)])?;
    let id = tx.last_insert_rowid();
    for (position, line) in document.concepts.iter().enumerate() {
        tx.execute(
            "INSERT INTO purchase_invoice_lines(invoice_id,position,concept_json) VALUES(?1,?2,?3)",
            params![id, position as i64, encode(line)?],
        )?;
    }
    insert_attachment(&tx, id, xml)?;
    if let Some(pdf) = pdf {
        insert_attachment(&tx, id, pdf)?;
    }
    let result = detail(&tx, id)?;
    tx.commit()?;
    Ok(InvoiceRegistrationResult {
        outcome: InvoiceRegistrationOutcome::Created,
        detail: result,
    })
}
pub fn attach(c: &mut Connection, id: i64, pdf: &CapturedFile) -> Result<PurchaseInvoiceDetail> {
    validate_id(id)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    detail(&tx, id)?;
    if let Some(existing) = attachment_metadata(&tx, id, InvoiceAttachmentKind::Pdf)? {
        if existing.sha256 != pdf.metadata.sha256 {
            return Err(AppError::conflict(
                "INVOICE_PDF_EXISTS",
                "La factura ya tiene otro PDF",
            ));
        }
    } else {
        insert_attachment(&tx, id, pdf)?;
    }
    let result = detail(&tx, id)?;
    tx.commit()?;
    Ok(result)
}
pub fn attachment(c: &Connection, id: i64, k: InvoiceAttachmentKind) -> Result<CapturedFile> {
    validate_id(id)?;
    if !c.query_row(
        "SELECT EXISTS(SELECT 1 FROM purchase_invoices WHERE id=?1)",
        [id],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(AppError::not_found(
            "INVOICE_NOT_FOUND",
            "Factura no encontrada",
        ));
    }
    let metadata = attachment_metadata(c, id, k)?.ok_or_else(|| {
        AppError::not_found("INVOICE_ATTACHMENT_NOT_FOUND", "Adjunto no encontrado")
    })?;
    let bytes: Vec<u8> = c.query_row(
        "SELECT content FROM purchase_invoice_attachments WHERE invoice_id=?1 AND kind=?2",
        params![id, kind(k)],
        |r| r.get(0),
    )?;
    let captured = super::purchase_invoice_files::capture(bytes, &metadata.original_name, k)
        .map_err(|_| AppError::database())?;
    if captured.metadata != metadata {
        return Err(AppError::database());
    }
    Ok(captured)
}
fn invalid_filter() -> AppError {
    AppError::validation("INVOICE_INVALID_FILTER", "Filtro de factura invalido")
}
pub fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return false;
    }
    let y = s[..4].parse::<u32>().unwrap_or(0);
    let m = s[5..7].parse::<u32>().unwrap_or(0);
    let d = s[8..].parse::<u32>().unwrap_or(0);
    let max = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    y > 0 && d > 0 && d <= max
}
fn rfc(s: &str) -> Result<String> {
    let s = s.trim().to_uppercase();
    let chars: Vec<_> = s.chars().collect();
    if !(12..=13).contains(&chars.len()) {
        return Err(invalid_filter());
    }
    let n = chars.len() - 9;
    if !chars[..n]
        .iter()
        .all(|c| c.is_ascii_uppercase() || matches!(c, 'Ñ' | '&'))
        || !chars[n..n + 6].iter().all(char::is_ascii_digit)
        || !chars[n + 6..].iter().all(char::is_ascii_alphanumeric)
    {
        return Err(invalid_filter());
    }
    Ok(s)
}
fn like(s: &str) -> String {
    format!(
        "%{}%",
        s.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}
pub fn view(c: &Connection, f: PurchaseInvoicesViewFilters) -> Result<PurchaseInvoicesView> {
    let page = f.page.unwrap_or(1);
    if page == 0 {
        return Err(invalid_filter());
    }
    for date in [&f.date_from, &f.date_to].into_iter().flatten() {
        if !valid_date(date) {
            return Err(invalid_filter());
        }
    }
    if matches!((&f.date_from,&f.date_to),(Some(a),Some(b)) if a>b) {
        return Err(invalid_filter());
    }
    let supplier = f.supplier_rfc.as_deref().map(rfc).transpose()?;
    let number = like(&normalize_search(f.number)?);
    let where_sql = r"(?1 IS NULL OR i.issuer_rfc=?1) AND (i.number_search_key LIKE ?2 ESCAPE '\' OR lower(i.uuid) LIKE ?2 ESCAPE '\') AND (?3 IS NULL OR i.issued_date>=?3) AND (?4 IS NULL OR i.issued_date<=?4)";
    let total = c.query_row(
        &format!("SELECT count(*) FROM purchase_invoices i WHERE {where_sql}"),
        params![supplier, number, f.date_from, f.date_to],
        |r| r.get(0),
    )?;
    let invoices=c.prepare(&format!("SELECT i.id,i.uuid,i.issuer_rfc,i.issuer_name,i.series,i.folio,i.issued_at,i.issued_date,i.currency,i.total,EXISTS(SELECT 1 FROM purchase_invoice_attachments a WHERE a.invoice_id=i.id AND a.kind='pdf') FROM purchase_invoices i WHERE {where_sql} ORDER BY i.issued_date DESC,i.issued_at DESC,i.id DESC LIMIT 25 OFFSET ?5"))?.query_map(params![supplier,number,f.date_from,f.date_to,i64::from(page-1)*25],|r|Ok(PurchaseInvoiceSummary{id:r.get(0)?,uuid:r.get(1)?,issuer:InvoiceParty{rfc:r.get(2)?,name:r.get(3)?},series:r.get(4)?,folio:r.get(5)?,issued_at:r.get(6)?,issued_date:r.get(7)?,currency:r.get(8)?,total:r.get(9)?,has_pdf:r.get(10)?}))?.collect::<std::result::Result<_,_>>()?;
    Ok(PurchaseInvoicesView {
        invoices,
        total,
        page,
        page_size: PURCHASE_INVOICE_PAGE_SIZE,
    })
}
pub fn suppliers(c: &Connection, search: Option<String>) -> Result<Vec<InvoiceSupplier>> {
    let normalized = normalize_search(search)?;
    let term = like(&normalized);
    let rfc_term = like(&normalized.to_uppercase());
    Ok(c.prepare(r"SELECT i.issuer_rfc,i.issuer_name FROM purchase_invoices i WHERE NOT EXISTS(SELECT 1 FROM purchase_invoices n WHERE n.issuer_rfc=i.issuer_rfc AND (n.issued_at>i.issued_at OR (n.issued_at=i.issued_at AND n.id>i.id))) AND EXISTS(SELECT 1 FROM purchase_invoices h WHERE h.issuer_rfc=i.issuer_rfc AND (h.issuer_search_key LIKE ?1 ESCAPE '\' OR h.issuer_rfc LIKE ?2 ESCAPE '\')) ORDER BY i.issuer_search_key,i.issuer_rfc")?.query_map(params![term,rfc_term],|r|Ok(InvoiceSupplier{rfc:r.get(0)?,name:r.get(1)?}))?.collect::<std::result::Result<_,_>>()?)
}
