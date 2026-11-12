/** Frozen IPC contract: see docs/PURCHASE_INVOICE_CONTRACT.md. Decimals stay lexical strings. */
export interface InvoiceParty { rfc: string; name: string }
export type InvoiceTaxKind = "transferred" | "withheld";
export interface InvoiceTax {
  kind: InvoiceTaxKind;
  code: string;
  base: string | null;
  factor_type: string | null;
  rate: string | null;
  amount: string | null;
}
export interface InvoiceConcept {
  supplier_code: string | null;
  product_service_code: string;
  unit_code: string;
  unit: string | null;
  description: string;
  quantity: string;
  unit_price: string;
  amount: string;
  discount: string | null;
  taxes: InvoiceTax[];
}
export interface InvoiceDocument {
  version: string;
  document_type: string;
  issuer: InvoiceParty;
  receiver: InvoiceParty;
  uuid: string;
  series: string | null;
  folio: string | null;
  issued_at: string;
  issued_date: string;
  stamped_at: string | null;
  currency: string;
  exchange_rate: string | null;
  subtotal: string;
  discount: string | null;
  total_transferred_taxes: string | null;
  total_withheld_taxes: string | null;
  taxes: InvoiceTax[];
  total: string;
  concepts: InvoiceConcept[];
}
export type InvoiceAttachmentKind = "xml" | "pdf";
export interface InvoiceAttachmentMetadata {
  kind: InvoiceAttachmentKind;
  original_name: string;
  mime_type: string;
  size_bytes: number;
  sha256: string;
}
export interface PurchaseInvoicePreview {
  review_token: string;
  document: InvoiceDocument;
  xml: InvoiceAttachmentMetadata;
  pdf: InvoiceAttachmentMetadata | null;
}
export interface PurchaseInvoiceAudit {
  registered_at: string;
  registered_by_user_id: number;
  registered_by_name: string;
}
export interface PurchaseInvoiceDetail {
  id: number;
  document: InvoiceDocument;
  audit: PurchaseInvoiceAudit;
  xml: InvoiceAttachmentMetadata;
  pdf: InvoiceAttachmentMetadata | null;
}
export interface PurchaseInvoiceSummary {
  id: number;
  uuid: string;
  issuer: InvoiceParty;
  series: string | null;
  folio: string | null;
  issued_at: string;
  issued_date: string;
  currency: string;
  total: string;
  has_pdf: boolean;
}
export interface PurchaseInvoicesViewFilters {
  supplier_rfc?: string | null;
  number?: string | null;
  date_from?: string | null;
  date_to?: string | null;
  page?: number | null;
}
export interface PurchaseInvoicesView {
  invoices: PurchaseInvoiceSummary[];
  total: number;
  page: number;
  page_size: number;
}
export interface InvoiceSupplier { rfc: string; name: string }
export type InvoiceRegistrationOutcome = "created" | "existing";
export interface InvoiceRegistrationResult {
  outcome: InvoiceRegistrationOutcome;
  detail: PurchaseInvoiceDetail;
}
export const PURCHASE_INVOICE_PAGE_SIZE = 25;
