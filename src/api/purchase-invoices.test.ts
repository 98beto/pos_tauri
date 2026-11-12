import { beforeEach, describe, expect, it } from "vitest";
import { invokeMock } from "../test/setup";
import {
  attachInvoicePdf, discardInvoicePreview, exportInvoiceAttachment,
  getPurchaseInvoiceDetail, getPurchaseInvoicesView, openInvoicePdf,
  pickInvoicePreviewPdf, pickInvoiceXml, registerInvoice, searchInvoiceSuppliers,
} from "./purchase-invoices";

beforeEach(() => invokeMock.mockReset().mockResolvedValue(undefined));

describe("frozen purchase invoice adapters", () => {
  it("uses camelCase outer arguments and unmodified snake_case filters, never paths or bytes", async () => {
    const filters = { supplier_rfc: "AAA010101AAA", number: "Á-0007", date_from: "2024-02-29", date_to: "2024-03-01", page: 2 };
    await pickInvoiceXml();
    await pickInvoicePreviewPdf("opaque-token");
    await discardInvoicePreview("opaque-token");
    await registerInvoice("opaque-token");
    await getPurchaseInvoicesView(filters);
    await getPurchaseInvoicesView();
    await searchInvoiceSuppliers("Café");
    await searchInvoiceSuppliers();
    await getPurchaseInvoiceDetail(42);
    await attachInvoicePdf(42);
    await exportInvoiceAttachment(42, "xml");
    await exportInvoiceAttachment(42, "pdf");
    await openInvoicePdf(42);
    expect(invokeMock.mock.calls).toEqual([
      ["pick_invoice_xml"],
      ["pick_invoice_preview_pdf", { reviewToken: "opaque-token" }],
      ["discard_invoice_preview", { reviewToken: "opaque-token" }],
      ["register_invoice", { reviewToken: "opaque-token" }],
      ["get_purchase_invoices_view", { filters }],
      ["get_purchase_invoices_view", { filters: {} }],
      ["search_invoice_suppliers", { search: "Café" }],
      ["search_invoice_suppliers", { search: undefined }],
      ["get_purchase_invoice_detail", { invoiceId: 42 }],
      ["attach_invoice_pdf", { invoiceId: 42 }],
      ["export_invoice_attachment", { invoiceId: 42, kind: "xml" }],
      ["export_invoice_attachment", { invoiceId: 42, kind: "pdf" }],
      ["open_invoice_pdf", { invoiceId: 42 }],
    ]);
  });
  it("preserves cancellation and export cancellation rather than fabricating state", async () => {
    invokeMock.mockResolvedValueOnce(null).mockResolvedValueOnce(null).mockResolvedValueOnce(null).mockResolvedValueOnce(false);
    await expect(pickInvoiceXml()).resolves.toBeNull();
    await expect(pickInvoicePreviewPdf("review")).resolves.toBeNull();
    await expect(attachInvoicePdf(1)).resolves.toBeNull();
    await expect(exportInvoiceAttachment(1, "xml")).resolves.toBe(false);
  });
  it("passes the document, lexical decimals, nulls and existing outcome through untouched", async () => {
    const detail = { id: 42, document: { total: "00010.123456", issued_at: "2024-02-29T23:59:59", discount: null } };
    const result = { outcome: "existing", detail };
    invokeMock.mockResolvedValueOnce(result).mockResolvedValueOnce(detail);
    await expect(registerInvoice("review")).resolves.toBe(result);
    await expect(getPurchaseInvoiceDetail(42)).resolves.toBe(detail);
  });
  it("propagates structured backend errors without converting them to cancellation", async () => {
    const error = { category: "conflict", code: "INVOICE_CONTENT_DIFFERENT", message: "El UUID ya existe con otro XML" };
    invokeMock.mockRejectedValueOnce(error);
    await expect(registerInvoice("review")).rejects.toBe(error);
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });
});
