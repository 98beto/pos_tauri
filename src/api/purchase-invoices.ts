// Adapters for authenticated native commands; no paths or bytes cross IPC.
import { invoke } from "@tauri-apps/api/core";
import type {
  InvoiceAttachmentKind, InvoiceRegistrationResult, InvoiceSupplier,
  PurchaseInvoiceDetail, PurchaseInvoicePreview, PurchaseInvoicesView,
  PurchaseInvoicesViewFilters,
} from "../types/purchase_invoice";

export function pickInvoiceXml(): Promise<PurchaseInvoicePreview | null> {
  return invoke("pick_invoice_xml");
}
export function pickInvoicePreviewPdf(reviewToken: string): Promise<PurchaseInvoicePreview | null> {
  return invoke("pick_invoice_preview_pdf", { reviewToken });
}
export function discardInvoicePreview(reviewToken: string): Promise<void> {
  return invoke("discard_invoice_preview", { reviewToken });
}
export function registerInvoice(reviewToken: string): Promise<InvoiceRegistrationResult> {
  return invoke("register_invoice", { reviewToken });
}
export function getPurchaseInvoicesView(filters: PurchaseInvoicesViewFilters = {}): Promise<PurchaseInvoicesView> {
  return invoke("get_purchase_invoices_view", { filters });
}
export function searchInvoiceSuppliers(search?: string): Promise<InvoiceSupplier[]> {
  return invoke("search_invoice_suppliers", { search });
}
export function getPurchaseInvoiceDetail(invoiceId: number): Promise<PurchaseInvoiceDetail> {
  return invoke("get_purchase_invoice_detail", { invoiceId });
}
export function attachInvoicePdf(invoiceId: number): Promise<PurchaseInvoiceDetail | null> {
  return invoke("attach_invoice_pdf", { invoiceId });
}
export function exportInvoiceAttachment(invoiceId: number, kind: InvoiceAttachmentKind): Promise<boolean> {
  return invoke("export_invoice_attachment", { invoiceId, kind });
}
export function openInvoicePdf(invoiceId: number): Promise<void> {
  return invoke("open_invoice_pdf", { invoiceId });
}
