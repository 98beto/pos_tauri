//! Frozen documentary DTOs. Decimal strings are never POS cents or stock quantities.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceParty {
    pub rfc: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceTaxKind {
    Transferred,
    Withheld,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceTax {
    pub kind: InvoiceTaxKind,
    pub code: String,
    pub base: Option<String>,
    pub factor_type: Option<String>,
    pub rate: Option<String>,
    pub amount: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceConcept {
    pub supplier_code: Option<String>,
    pub product_service_code: String,
    pub unit_code: String,
    pub unit: Option<String>,
    pub description: String,
    pub quantity: String,
    pub unit_price: String,
    pub amount: String,
    pub discount: Option<String>,
    pub taxes: Vec<InvoiceTax>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceDocument {
    pub version: String,
    pub document_type: String,
    pub issuer: InvoiceParty,
    pub receiver: InvoiceParty,
    pub uuid: String,
    pub series: Option<String>,
    pub folio: Option<String>,
    pub issued_at: String,
    pub issued_date: String,
    pub stamped_at: Option<String>,
    pub currency: String,
    pub exchange_rate: Option<String>,
    pub subtotal: String,
    pub discount: Option<String>,
    pub total_transferred_taxes: Option<String>,
    pub total_withheld_taxes: Option<String>,
    pub taxes: Vec<InvoiceTax>,
    pub total: String,
    pub concepts: Vec<InvoiceConcept>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceAttachmentKind {
    Xml,
    Pdf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceAttachmentMetadata {
    pub kind: InvoiceAttachmentKind,
    pub original_name: String,
    pub mime_type: String,
    pub size_bytes: u32,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseInvoicePreview {
    pub review_token: String,
    pub document: InvoiceDocument,
    pub xml: InvoiceAttachmentMetadata,
    pub pdf: Option<InvoiceAttachmentMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseInvoiceAudit {
    pub registered_at: String,
    pub registered_by_user_id: i64,
    pub registered_by_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseInvoiceDetail {
    pub id: i64,
    pub document: InvoiceDocument,
    pub audit: PurchaseInvoiceAudit,
    pub xml: InvoiceAttachmentMetadata,
    pub pdf: Option<InvoiceAttachmentMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseInvoiceSummary {
    pub id: i64,
    pub uuid: String,
    pub issuer: InvoiceParty,
    pub series: Option<String>,
    pub folio: Option<String>,
    pub issued_at: String,
    pub issued_date: String,
    pub currency: String,
    pub total: String,
    pub has_pdf: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseInvoicesViewFilters {
    pub supplier_rfc: Option<String>,
    pub number: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub page: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseInvoicesView {
    pub invoices: Vec<PurchaseInvoiceSummary>,
    pub total: u32,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceSupplier {
    pub rfc: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceRegistrationOutcome {
    Created,
    Existing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceRegistrationResult {
    pub outcome: InvoiceRegistrationOutcome,
    pub detail: PurchaseInvoiceDetail,
}

pub const PURCHASE_INVOICE_PAGE_SIZE: u32 = 25;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::purchase_invoice_parser::parse_purchase_invoice;

    #[test]
    fn purchase_invoice_envelopes_serialize_without_bytes_paths_or_numeric_decimals() {
        let document =
            parse_purchase_invoice(include_bytes!("../../tests/fixtures/cfdi/income.xml")).unwrap();
        let xml = InvoiceAttachmentMetadata {
            kind: InvoiceAttachmentKind::Xml,
            original_name: "income.xml".into(),
            mime_type: "application/xml".into(),
            size_bytes: 123,
            sha256: "a".repeat(64),
        };
        let preview = PurchaseInvoicePreview {
            review_token: "opaque".into(),
            document: document.clone(),
            xml: xml.clone(),
            pdf: None,
        };
        let value = serde_json::to_value(&preview).unwrap();
        assert_eq!(
            value
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["document", "pdf", "review_token", "xml"]
        );
        assert!(value["pdf"].is_null());
        assert_eq!(value["document"]["subtotal"], "0010.123456");
        assert_eq!(value["xml"].as_object().unwrap().len(), 5);
        assert_eq!(
            serde_json::from_value::<PurchaseInvoicePreview>(value).unwrap(),
            preview
        );
        let detail = PurchaseInvoiceDetail {
            id: 1,
            document: document.clone(),
            xml,
            pdf: None,
            audit: PurchaseInvoiceAudit {
                registered_at: "2026-11-06T00:00:00Z".into(),
                registered_by_user_id: 2,
                registered_by_name: "Operador".into(),
            },
        };
        let result = InvoiceRegistrationResult {
            outcome: InvoiceRegistrationOutcome::Created,
            detail,
        };
        let value = serde_json::to_value(&result).unwrap();
        assert_eq!(value["outcome"], "created");
        assert_eq!(value["detail"]["audit"]["registered_by_user_id"], 2);
        assert_eq!(
            serde_json::from_value::<InvoiceRegistrationResult>(value).unwrap(),
            result
        );
        let view = PurchaseInvoicesView {
            invoices: vec![PurchaseInvoiceSummary {
                id: 1,
                uuid: document.uuid,
                issuer: document.issuer,
                series: document.series,
                folio: document.folio,
                issued_at: document.issued_at,
                issued_date: document.issued_date,
                currency: document.currency,
                total: document.total,
                has_pdf: false,
            }],
            total: 1,
            page: 1,
            page_size: PURCHASE_INVOICE_PAGE_SIZE,
        };
        let value = serde_json::to_value(&view).unwrap();
        assert_eq!(value["page_size"], 25);
        let summary = value["invoices"][0].as_object().unwrap();
        assert_eq!(summary.len(), 10);
        for excluded in [
            "document", "concepts", "xml", "pdf", "taxes", "content", "bytes",
        ] {
            assert!(!summary.contains_key(excluded));
        }
        assert_eq!(
            serde_json::from_value::<PurchaseInvoicesView>(value).unwrap(),
            view
        );
        let supplier = InvoiceSupplier {
            rfc: "AAA010101AAA".into(),
            name: "Café".into(),
        };
        let value = serde_json::to_value(&supplier).unwrap();
        assert_eq!(
            serde_json::from_value::<InvoiceSupplier>(value).unwrap(),
            supplier
        );
    }

    #[test]
    fn purchase_invoice_filters_allow_omitted_or_null_properties() {
        let empty = serde_json::from_str::<PurchaseInvoicesViewFilters>("{}").unwrap();
        assert_eq!(empty, PurchaseInvoicesViewFilters::default());
        let filters: PurchaseInvoicesViewFilters = serde_json::from_str(
            r#"{"supplier_rfc":null,"number":"Á-0007","date_from":"2024-02-29","page":2}"#,
        )
        .unwrap();
        assert_eq!(filters.page, Some(2));
        let value = serde_json::to_value(&filters).unwrap();
        assert!(value["date_to"].is_null());
        assert_eq!(value["number"], "Á-0007");
        assert_eq!(
            serde_json::from_value::<PurchaseInvoicesViewFilters>(value).unwrap(),
            filters
        );
    }
}
