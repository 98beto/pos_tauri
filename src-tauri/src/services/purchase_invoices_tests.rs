use super::*;
use crate::{
    db::migrations,
    services::{purchase_invoice_files::capture, purchase_invoice_parser::parse_purchase_invoice},
};
const XML: &[u8] = include_bytes!("../../tests/fixtures/cfdi/income.xml");
fn user() -> User {
    User {
        id: 1,
        first_name: "Ada".into(),
        last_name: "POS".into(),
        email: "a@b".into(),
        created_at: "".into(),
        updated_at: "".into(),
    }
}
fn setup() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    migrations::run(&mut c).unwrap();
    c.pragma_update(None, "foreign_keys", true).unwrap();
    c.execute("INSERT INTO users(id,first_name,last_name,email,password_hash) VALUES(1,'Ada','POS','a@b','hash')",[]).unwrap();
    c
}
fn xml(bytes: &[u8]) -> CapturedFile {
    capture(
        bytes.to_vec(),
        "../original.xml",
        InvoiceAttachmentKind::Xml,
    )
    .unwrap()
}
fn save(
    c: &mut Connection,
    file: &CapturedFile,
    pdf: Option<&CapturedFile>,
) -> InvoiceRegistrationResult {
    register(
        c,
        &user(),
        &parse_purchase_invoice(&file.bytes).unwrap(),
        file,
        pdf,
    )
    .unwrap()
}
#[test]
fn purchase_invoice_original_lines_duplicate_conflict_and_attachment_only_missing() {
    let mut c = setup();
    let file = xml(XML);
    let first = save(&mut c, &file, None);
    assert_eq!(first.outcome, InvoiceRegistrationOutcome::Created);
    assert_eq!(first.detail.document, parse_purchase_invoice(XML).unwrap());
    assert!(first.detail.audit.registered_at.ends_with('Z'));
    assert_eq!(first.detail.audit.registered_by_name, "Ada POS");
    let pdf = capture(
        b"%PDF-original".to_vec(),
        "a.pdf",
        InvoiceAttachmentKind::Pdf,
    )
    .unwrap();
    let retry = save(&mut c, &file, Some(&pdf));
    assert_eq!(retry.outcome, InvoiceRegistrationOutcome::Existing);
    assert!(retry.detail.pdf.is_none());
    let different = xml(&[XML, b"\n"].concat());
    assert_eq!(
        register(
            &mut c,
            &user(),
            &parse_purchase_invoice(&different.bytes).unwrap(),
            &different,
            None
        )
        .unwrap_err()
        .code,
        "INVOICE_CONTENT_DIFFERENT"
    );
    assert_eq!(
        attachment(&c, first.detail.id, InvoiceAttachmentKind::Xml)
            .unwrap()
            .bytes,
        file.bytes
    );
    let attached = attach(&mut c, first.detail.id, &pdf).unwrap();
    assert_eq!(attach(&mut c, first.detail.id, &pdf).unwrap(), attached);
    let other = capture(b"%PDF-other".to_vec(), "b.pdf", InvoiceAttachmentKind::Pdf).unwrap();
    assert_eq!(
        attach(&mut c, first.detail.id, &other).unwrap_err().code,
        "INVOICE_PDF_EXISTS"
    );
    assert_eq!(
        attachment(&c, first.detail.id, InvoiceAttachmentKind::Pdf)
            .unwrap()
            .bytes,
        pdf.bytes
    );
}
#[test]
fn purchase_invoice_failure_after_line_rolls_back_everything() {
    let mut c = setup();
    c.execute_batch("CREATE TRIGGER fail_second_line BEFORE INSERT ON purchase_invoice_lines WHEN NEW.position=1 BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    let f = xml(XML);
    assert_eq!(
        register(
            &mut c,
            &user(),
            &parse_purchase_invoice(XML).unwrap(),
            &f,
            None
        )
        .unwrap_err()
        .code,
        "DATABASE_ERROR"
    );
    for table in [
        "purchase_invoices",
        "purchase_invoice_lines",
        "purchase_invoice_attachments",
    ] {
        assert_eq!(
            c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn purchase_invoice_number_search_separates_visible_number_from_uuid() {
    let mut c = setup();
    let file = xml(XML);
    let mut document = parse_purchase_invoice(XML).unwrap();
    register(&mut c, &user(), &document, &file, None).unwrap();

    document.uuid = "11234567-89AB-CDEF-0123-456789ABCDEF".into();
    document.series = Some("INV".into());
    document.folio = None;
    register(&mut c, &user(), &document, &file, None).unwrap();

    document.uuid = "21234567-89AB-CDEF-0123-456789ABCDEF".into();
    document.series = None;
    document.folio = Some("SOLO".into());
    register(&mut c, &user(), &document, &file, None).unwrap();

    document.uuid = "31234567-89AB-CDEF-0123-456789ABCDEF".into();
    document.folio = None;
    register(&mut c, &user(), &document, &file, None).unwrap();

    let keys: Vec<String> = c
        .prepare("SELECT number_search_key FROM purchase_invoices ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(
        keys,
        [
            "á-0007",
            "inv",
            "solo",
            "31234567-89ab-cdef-0123-456789abcdef"
        ]
    );

    for (term, expected) in [
        ("á", 1),
        ("0007", 1),
        ("Á-0007", 1),
        ("01234567-89ab", 1),
        ("inv", 1),
        ("solo", 1),
        ("31234567", 1),
        ("0007 01234567", 0),
        ("inv-", 0),
        ("%", 0),
        ("_", 0),
    ] {
        for filtered in [false, true] {
            let filters = PurchaseInvoicesViewFilters {
                number: Some(term.into()),
                supplier_rfc: filtered.then(|| "AAA010101AAA".into()),
                date_from: filtered.then(|| "2024-02-29".into()),
                date_to: filtered.then(|| "2024-02-29".into()),
                ..Default::default()
            };
            let result = view(&c, filters).unwrap();
            assert_eq!(result.total, expected, "term {term:?}, filtered {filtered}");
            assert_eq!(result.invoices.len(), expected as usize);
        }
    }

    for filters in [
        PurchaseInvoicesViewFilters {
            supplier_rfc: Some("BBB010101BBB".into()),
            ..Default::default()
        },
        PurchaseInvoicesViewFilters {
            date_from: Some("2024-03-01".into()),
            ..Default::default()
        },
        PurchaseInvoicesViewFilters {
            date_to: Some("2024-02-28".into()),
            ..Default::default()
        },
    ] {
        for term in ["0007", "01234567-89ab"] {
            assert_eq!(
                view(
                    &c,
                    PurchaseInvoicesViewFilters {
                        number: Some(term.into()),
                        ..filters.clone()
                    }
                )
                .unwrap()
                .total,
                0
            );
        }
    }
}
#[test]
fn purchase_invoice_filters_pagination_supplier_history_and_no_blob_reads() {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
    let mut c = setup();
    for i in 0..28 {
        let text = String::from_utf8(XML.to_vec())
            .unwrap()
            .replace(
                "01234567-89ab-cdef-0123-456789abcdef",
                &format!("{i:08x}-89ab-cdef-0123-456789abcdef"),
            )
            .replace(
                "Café y Té S.A.",
                if i == 27 {
                    "Último nombre"
                } else {
                    "Café histórico"
                },
            );
        save(&mut c, &xml(text.as_bytes()), None);
    }
    c.authorizer(Some(|ctx: AuthContext<'_>| match ctx.action {
        AuthAction::Read {
            table_name: "purchase_invoice_attachments",
            column_name: "content",
        }
        | AuthAction::Read {
            table_name: "purchase_invoice_lines",
            ..
        } => Authorization::Deny,
        _ => Authorization::Allow,
    }))
    .unwrap();
    let f = PurchaseInvoicesViewFilters {
        supplier_rfc: Some(" aaa010101aaa ".into()),
        number: Some(" á-0007 ".into()),
        date_from: Some("2024-02-29".into()),
        date_to: Some("2024-02-29".into()),
        page: None,
    };
    let v = view(&c, f.clone()).unwrap();
    assert_eq!(v.total, 28);
    assert_eq!(v.invoices.len(), 25);
    assert_eq!(v.invoices[0].id, 28);
    let v2 = view(
        &c,
        PurchaseInvoicesViewFilters {
            page: Some(2),
            ..f.clone()
        },
    )
    .unwrap();
    assert_eq!(v2.invoices.len(), 3);
    assert_eq!(v2.invoices[0].id, 3);
    assert!(view(
        &c,
        PurchaseInvoicesViewFilters {
            page: Some(u32::MAX),
            ..f.clone()
        }
    )
    .unwrap()
    .invoices
    .is_empty());
    assert_eq!(
        suppliers(&c, Some("CAFÉ HISTÓRICO".into())).unwrap(),
        vec![InvoiceSupplier {
            rfc: "AAA010101AAA".into(),
            name: "Último nombre".into()
        }]
    );
    for term in ["%", "_", "\\"] {
        assert_eq!(
            view(
                &c,
                PurchaseInvoicesViewFilters {
                    number: Some(term.into()),
                    ..Default::default()
                }
            )
            .unwrap()
            .total,
            0
        );
        assert!(suppliers(&c, Some(term.into())).unwrap().is_empty());
    }
    assert_eq!(
        view(
            &c,
            PurchaseInvoicesViewFilters {
                number: Some("0000001b-89AB".into()),
                ..Default::default()
            }
        )
        .unwrap()
        .total,
        1
    );
    for date in [
        "2023-02-29",
        "0000-01-01",
        "2024-13-01",
        "2024-02-30",
        "２０２４-01-01",
    ] {
        assert_eq!(
            view(
                &c,
                PurchaseInvoicesViewFilters {
                    date_from: Some(date.into()),
                    ..Default::default()
                }
            )
            .unwrap_err()
            .code,
            "INVOICE_INVALID_FILTER"
        );
    }
    assert_eq!(
        view(
            &c,
            PurchaseInvoicesViewFilters {
                page: Some(0),
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        "INVOICE_INVALID_FILTER"
    );
    assert_eq!(
        view(
            &c,
            PurchaseInvoicesViewFilters {
                number: Some("a".repeat(101)),
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        "SEARCH_TOO_LONG"
    );
    assert_eq!(
        view(
            &c,
            PurchaseInvoicesViewFilters {
                date_from: Some("2024-03-01".into()),
                date_to: Some("2024-02-29".into()),
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        "INVOICE_INVALID_FILTER"
    );
    assert_eq!(detail(&c, 0).unwrap_err().code, "INVALID_ID");
    c.authorizer(Some(|ctx: AuthContext<'_>| match ctx.action {
        AuthAction::Read {
            table_name: "purchase_invoice_attachments",
            column_name: "content",
        } => Authorization::Deny,
        _ => Authorization::Allow,
    }))
    .unwrap();
    assert_eq!(
        detail(&c, 28).unwrap().document.concepts,
        parse_purchase_invoice(XML).unwrap().concepts
    );
}

#[test]
fn purchase_invoice_supplier_representative_uses_emission_not_last_insert_and_unicode_rfc() {
    let mut c = setup();
    let text = String::from_utf8(XML.to_vec()).unwrap();
    let latest = text.replace("aaa010101aaa", "ñaa010101aaa");
    save(&mut c, &xml(latest.as_bytes()), None);
    let older = latest
        .replace(
            "01234567-89ab-cdef-0123-456789abcdef",
            "11234567-89ab-cdef-0123-456789abcdef",
        )
        .replace("2024-02-29T23:59:59", "2023-01-01T00:00:00")
        .replace("Café y Té S.A.", "Nombre histórico");
    save(&mut c, &xml(older.as_bytes()), None);
    for term in ["ñaa", "NOMBRE HISTÓRICO", "café"] {
        assert_eq!(
            suppliers(&c, Some(term.into())).unwrap(),
            vec![InvoiceSupplier {
                rfc: "ÑAA010101AAA".into(),
                name: "Café y Té S.A.".into()
            }]
        );
    }
}
#[test]
fn purchase_invoice_uuid_concurrency_is_serialized_by_immediate_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("race.db");
    let mut c = Connection::open(&path).unwrap();
    migrations::run(&mut c).unwrap();
    c.execute("INSERT INTO users(id,first_name,last_name,email,password_hash) VALUES(1,'Ada','POS','a@b','h')",[]).unwrap();
    drop(c);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut c = Connection::open(path).unwrap();
                c.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
                barrier.wait();
                save(&mut c, &xml(XML), None)
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(
        results
            .iter()
            .filter(|r| r.outcome == InvoiceRegistrationOutcome::Created)
            .count(),
        1
    );
    assert_eq!(results[0].detail, results[1].detail);
}
