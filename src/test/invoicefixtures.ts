import type { InvoiceConcept, InvoiceDocument, PurchaseInvoiceDetail, PurchaseInvoicePreview, PurchaseInvoicesView } from "../types/purchase_invoice";

export const invoiceConcept: InvoiceConcept = {
  supplier_code: null, product_service_code: "01010101", unit_code: "H87", unit: "Pieza original", description: "Descripción completa del proveedor",
  quantity: "2.000", unit_price: "999999999999999999.123456", amount: "0010.123456", discount: null,
  taxes: [{ kind: "transferred", code: "002", base: "10.000000", factor_type: "Exento", rate: null, amount: null }],
};
export const invoiceDocument: InvoiceDocument = {
  version: "4.0", document_type: "I", issuer: { rfc: "AAA010101AAA", name: "Proveedor original" }, receiver: { rfc: "BBB010101BBB", name: "Receptor original" },
  uuid: "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE", series: "A", folio: "42", issued_at: "2026-01-31T23:59:59", issued_date: "2026-01-31", stamped_at: null,
  currency: "USD", exchange_rate: "20.000000", subtotal: "0010.123456", discount: "0", total_transferred_taxes: null, total_withheld_taxes: null, taxes: [], total: "0010.123456", concepts: [invoiceConcept],
};
export const invoiceXml = { kind: "xml" as const, original_name: "original.xml", mime_type: "application/xml", size_bytes: 1200, sha256: "a".repeat(64) };
export const invoicePdf = { ...invoiceXml, kind: "pdf" as const, original_name: "usuario.pdf", mime_type: "application/pdf" };
export const invoicePreview: PurchaseInvoicePreview = { review_token: "opaque-review-token", document: invoiceDocument, xml: invoiceXml, pdf: null };
export const invoiceDetail: PurchaseInvoiceDetail = { id: 7, document: invoiceDocument, audit: { registered_at: "2026-02-01T04:00:00Z", registered_by_user_id: 1, registered_by_name: "Operador" }, xml: invoiceXml, pdf: null };
export const invoiceCollection: PurchaseInvoicesView = { invoices: [{ id: 7, uuid: invoiceDocument.uuid, issuer: invoiceDocument.issuer, series: "A", folio: "42", issued_at: invoiceDocument.issued_at, issued_date: invoiceDocument.issued_date, currency: "USD", total: invoiceDocument.total, has_pdf: false }], total: 26, page: 1, page_size: 25 };
