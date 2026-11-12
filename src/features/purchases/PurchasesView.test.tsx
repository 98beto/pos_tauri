import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../../api/purchase-invoices";
import { invoiceCollection, invoiceDetail, invoicePdf, invoicePreview } from "../../test/invoicefixtures";
import { PurchasesView } from "./PurchasesView";

vi.mock("../../api/purchase-invoices", () => ({
  getPurchaseInvoicesView: vi.fn(), searchInvoiceSuppliers: vi.fn(), getPurchaseInvoiceDetail: vi.fn(), pickInvoiceXml: vi.fn(),
  pickInvoicePreviewPdf: vi.fn(), discardInvoicePreview: vi.fn(), registerInvoice: vi.fn(), attachInvoicePdf: vi.fn(), exportInvoiceAttachment: vi.fn(), openInvoicePdf: vi.fn(),
}));
const mock = vi.mocked(api);
const sessionError = { category: "authentication", code: "SESSION_REQUIRED", message: "Sesión requerida" };
const failed = { category: "internal", code: "DATABASE_ERROR", message: "Fallo controlado" };
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (reason: unknown) => void; const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
async function tick() { await act(async () => { await vi.advanceTimersByTimeAsync(260); }); }
async function click(name: string) { await act(async () => { fireEvent.click(screen.getByRole("button", { name })); }); }
async function start() { const auth = vi.fn(); const result = render(<PurchasesView sessionId={1} onSessionRequired={auth} />); await tick(); return { ...result, auth }; }
async function review() { await start(); await click("Importar factura"); }
async function detail() { await start(); await click("Ver detalle"); }

beforeEach(() => {
  vi.useFakeTimers(); vi.resetAllMocks(); window.localStorage.clear();
  mock.getPurchaseInvoicesView.mockResolvedValue(invoiceCollection);
  mock.searchInvoiceSuppliers.mockResolvedValue([invoiceDetail.document.issuer]);
  mock.getPurchaseInvoiceDetail.mockResolvedValue(invoiceDetail);
  mock.pickInvoiceXml.mockResolvedValue(invoicePreview);
  mock.pickInvoicePreviewPdf.mockResolvedValue(null);
  mock.discardInvoicePreview.mockResolvedValue(undefined);
  mock.registerInvoice.mockResolvedValue({ outcome: "created", detail: invoiceDetail });
  mock.attachInvoicePdf.mockResolvedValue(null);
  mock.exportInvoiceAttachment.mockResolvedValue(false);
  mock.openInvoicePdf.mockResolvedValue(undefined);
});
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });

describe("purchase collection", () => {
  it("combines AND filters, debounces, requests page 25 contract and prevents invalid ranges hiding a wrong list", async () => {
    await start();
    fireEvent.change(screen.getByLabelText("Proveedor"), { target: { value: invoiceDetail.document.issuer.rfc } });
    fireEvent.change(screen.getByLabelText("N.º de factura"), { target: { value: "A-4" } });
    fireEvent.change(screen.getByLabelText("Fecha de emisión desde"), { target: { value: "2026-01-01" } });
    fireEvent.change(screen.getByLabelText("Fecha de emisión hasta"), { target: { value: "2026-01-31" } });
    expect(mock.getPurchaseInvoicesView).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("26 facturas en resultados")).toBeNull();
    await tick();
    expect(mock.getPurchaseInvoicesView).toHaveBeenLastCalledWith({ supplier_rfc: "AAA010101AAA", number: "A-4", date_from: "2026-01-01", date_to: "2026-01-31", page: 1 });
    fireEvent.change(screen.getByLabelText("Fecha de emisión desde"), { target: { value: "2026-02-01" } });
    await tick();
    expect(screen.getByRole("alert").textContent).toContain("desde debe ser anterior");
    expect(screen.queryByRole("table")).toBeNull();
    expect(mock.getPurchaseInvoicesView).toHaveBeenCalledTimes(2);
    await click("Limpiar filtros"); await tick();
    expect(mock.getPurchaseInvoicesView).toHaveBeenLastCalledWith({ page: 1, supplier_rfc: undefined, number: undefined, date_from: undefined, date_to: undefined });
  });
  it("ignores obsolete provider results and auth errors, retaining an independently selected RFC", async () => {
    const old = deferred<Awaited<ReturnType<typeof api.searchInvoiceSuppliers>>>();
    mock.searchInvoiceSuppliers.mockReturnValueOnce(old.promise);
    const { auth } = await start();
    fireEvent.change(screen.getByLabelText("Buscar proveedor por nombre o RFC"), { target: { value: "nuevo" } });
    await tick();
    expect(mock.searchInvoiceSuppliers).toHaveBeenLastCalledWith("nuevo");
    await act(async () => old.reject(sessionError));
    expect(auth).not.toHaveBeenCalled();
    expect(screen.getByRole("option", { name: /Proveedor original/ })).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Proveedor"), { target: { value: "AAA010101AAA" } });
    mock.searchInvoiceSuppliers.mockResolvedValue([]);
    fireEvent.change(screen.getByLabelText("Buscar proveedor por nombre o RFC"), { target: { value: "otro" } });
    await tick();
    expect((screen.getByLabelText("Proveedor") as HTMLSelectElement).value).toBe("AAA010101AAA");
  });
  it("does not silently remove the date filter when the native control reports incomplete/invalid input", async () => {
    await start();
    const input = screen.getByLabelText("Fecha de emisión desde") as HTMLInputElement;
    Object.defineProperty(input, "validity", { configurable: true, value: { badInput: true } });
    fireEvent.input(input); await tick();
    expect(screen.getByRole("alert").textContent).toContain("fechas de emisión válidas");
    expect(screen.queryByRole("table")).toBeNull(); expect(mock.getPurchaseInvoicesView).toHaveBeenCalledTimes(1);
    await click("Limpiar filtros"); await tick(); expect(screen.getByRole("table")).toBeTruthy();
  });
  it("keeps the newest provider search when an older successful response arrives last", async () => {
    const old = deferred<Awaited<ReturnType<typeof api.searchInvoiceSuppliers>>>(); mock.searchInvoiceSuppliers.mockReturnValueOnce(old.promise);
    await start();
    fireEvent.change(screen.getByLabelText("Buscar proveedor por nombre o RFC"), { target: { value: "actual" } }); await tick();
    await act(async () => old.resolve([{ name: "Proveedor obsoleto", rfc: "OLD010101AAA" }]));
    expect(screen.queryByRole("option", { name: /Proveedor obsoleto/ })).toBeNull();
    expect(screen.getByRole("option", { name: /Proveedor original/ })).toBeTruthy();
  });
  it("retains filters/page through presentation and detail without refetching on mode changes", async () => {
    await start();
    fireEvent.change(screen.getByLabelText("N.º de factura"), { target: { value: "42" } }); await tick();
    mock.getPurchaseInvoicesView.mockResolvedValue({ ...invoiceCollection, page: 2 });
    await click("Siguiente"); await tick();
    const calls = mock.getPurchaseInvoicesView.mock.calls.length;
    await click("Tarjetas");
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.getByText("0010.123456 USD")).toBeTruthy();
    expect(window.localStorage.getItem("pos.view.purchases")).toBe("cards");
    await click("Ver detalle"); await click("Cerrar");
    expect(screen.getByText("Página 2 · 25 por página")).toBeTruthy();
    expect((screen.getByLabelText("N.º de factura") as HTMLInputElement).value).toBe("42");
    await click("Lista");
    expect(mock.getPurchaseInvoicesView).toHaveBeenCalledTimes(calls);
  });
  it.each(["invalid", "cards"])("restores independent preference %s", async (stored) => {
    window.localStorage.setItem("pos.view.purchases", stored);
    await start();
    expect(Boolean(screen.queryByRole("table"))).toBe(stored !== "cards");
  });
  it("falls back safely with denied storage", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("denied"); });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("denied"); });
    await start(); expect(screen.getByRole("table")).toBeTruthy(); await click("Tarjetas"); expect(screen.queryByRole("table")).toBeNull();
  });
  it("ignores obsolete list responses and separates empty/error/loading states", async () => {
    const old = deferred<typeof invoiceCollection>(); mock.getPurchaseInvoicesView.mockReturnValueOnce(old.promise);
    await start(); expect(screen.getByText("Cargando facturas…")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("N.º de factura"), { target: { value: "missing" } });
    mock.getPurchaseInvoicesView.mockResolvedValue({ ...invoiceCollection, invoices: [], total: 0 }); await tick();
    await act(async () => old.resolve(invoiceCollection));
    expect(screen.getByText("No hay facturas con estos filtros")).toBeTruthy();
    mock.getPurchaseInvoicesView.mockRejectedValue(failed);
    fireEvent.change(screen.getByLabelText("N.º de factura"), { target: { value: "other" } }); await tick();
    expect(screen.queryByText("0 facturas en resultados")).toBeNull(); expect(screen.getByText(/No se pudo actualizar el listado/)).toBeTruthy();
  });
});

describe("review and registration", () => {
  it("refreshes initially empty suppliers after the first invoice without changing the search", async () => {
    mock.searchInvoiceSuppliers.mockResolvedValueOnce([]);
    mock.getPurchaseInvoicesView.mockResolvedValueOnce({ ...invoiceCollection, invoices: [], total: 0 });
    await start();
    expect(screen.getByText("0 proveedores encontrados")).toBeTruthy();
    await click("Importar factura"); await click("Registrar factura"); await tick();
    expect(mock.searchInvoiceSuppliers).toHaveBeenCalledTimes(2);
    expect(mock.searchInvoiceSuppliers).toHaveBeenLastCalledWith(undefined);
    await click("Cerrar");
    expect(screen.getByRole("option", { name: /Proveedor original/ })).toBeTruthy();
  });
  it.each(["created", "existing"] as const)("refreshes supplier names after %s while retaining search, selection, filters and page", async (outcome) => {
    mock.registerInvoice.mockResolvedValue({ outcome, detail: invoiceDetail });
    await start();
    fireEvent.change(screen.getByLabelText("Proveedor"), { target: { value: "AAA010101AAA" } });
    fireEvent.change(screen.getByLabelText("Buscar proveedor por nombre o RFC"), { target: { value: "  proveedor  " } });
    fireEvent.change(screen.getByLabelText("N.º de factura"), { target: { value: "42" } });
    fireEvent.change(screen.getByLabelText("Fecha de emisión desde"), { target: { value: "2026-01-01" } });
    await tick();
    mock.getPurchaseInvoicesView.mockResolvedValue({ ...invoiceCollection, page: 2 });
    await click("Siguiente"); await tick();
    mock.searchInvoiceSuppliers.mockResolvedValue([{ ...invoiceDetail.document.issuer, name: "Nombre actualizado" }]);
    await click("Importar factura"); await click("Registrar factura"); await tick();
    await click("Cerrar");
    expect(mock.searchInvoiceSuppliers).toHaveBeenLastCalledWith("proveedor");
    expect((screen.getByLabelText("Buscar proveedor por nombre o RFC") as HTMLInputElement).value).toBe("  proveedor  ");
    expect((screen.getByLabelText("Proveedor") as HTMLSelectElement).value).toBe("AAA010101AAA");
    expect(screen.getByRole("option", { name: /Nombre actualizado/ })).toBeTruthy();
    expect(mock.getPurchaseInvoicesView).toHaveBeenLastCalledWith({ page: 2, supplier_rfc: "AAA010101AAA", number: "42", date_from: "2026-01-01" });
    expect(screen.getByText("Página 2 · 25 por página")).toBeTruthy();
  });
  it("treats native cancellation as normal and cancels preview without registering", async () => {
    mock.pickInvoiceXml.mockResolvedValueOnce(null); await start(); await click("Importar factura");
    expect(screen.queryByRole("dialog")).toBeNull(); expect(screen.queryByRole("alert")).toBeNull();
    await click("Importar factura"); await click("Cancelar revisión");
    expect(mock.discardInvoicePreview).toHaveBeenCalledWith(invoicePreview.review_token);
    expect(mock.registerInvoice).not.toHaveBeenCalled(); expect(screen.queryByRole("dialog")).toBeNull();
  });
  it("keeps the selected PDF on native cancellation and shares original concepts", async () => {
    mock.pickInvoiceXml.mockResolvedValue({ ...invoicePreview, pdf: invoicePdf }); await review();
    await click("Adjuntar PDF a revisión");
    expect(mock.pickInvoicePreviewPdf).toHaveBeenCalledWith(invoicePreview.review_token);
    expect(screen.getByText(/PDF adjuntado por el usuario: usuario.pdf/)).toBeTruthy();
    expect(screen.getByRole("table", { name: "Conceptos originales del proveedor" })).toBeTruthy();
    expect(screen.getByText("999999999999999999.123456 USD")).toBeTruthy();
  });
  it("locks double confirmation and modal close; response lost retries the same token", async () => {
    const pending = deferred<Awaited<ReturnType<typeof api.registerInvoice>>>(); mock.registerInvoice.mockReturnValueOnce(pending.promise);
    await review(); await click("Registrar factura");
    fireEvent.click(screen.getByRole("button", { name: "Registrar factura" })); fireEvent.keyDown(document, { key: "Escape" });
    expect(mock.registerInvoice).toHaveBeenCalledTimes(1); expect(screen.getByRole("dialog")).toBeTruthy();
    await act(async () => pending.reject(failed));
    expect(screen.getByRole("dialog", { name: "Revisar factura" })).toBeTruthy();
    await click("Registrar factura");
    expect(mock.registerInvoice.mock.calls).toEqual([[invoicePreview.review_token], [invoicePreview.review_token]]);
    expect(screen.getByRole("dialog", { name: "Detalle de factura" })).toBeTruthy();
  });
  it.each(["created", "existing"] as const)("shows confirmed %s detail even when collection refresh fails", async (outcome) => {
    mock.registerInvoice.mockResolvedValue({ outcome, detail: invoiceDetail }); await review();
    mock.getPurchaseInvoicesView.mockRejectedValue(failed); await click("Registrar factura"); await tick();
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText(outcome === "created" ? /Factura registrada/ : /ya estaba registrada/)).toBeTruthy();
    expect(within(dialog).getByText(/El documento sigue registrado/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Registrar factura" })).toBeNull();
    expect(mock.registerInvoice).toHaveBeenCalledTimes(1);
  });
  it("stale token requires an explicit new XML review rather than regeneration", async () => {
    mock.registerInvoice.mockRejectedValue({ ...failed, code: "INVOICE_PREVIEW_STALE" }); await review(); await click("Registrar factura");
    expect((screen.getByRole("button", { name: "Registrar factura" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText(/vuelve a seleccionar el XML/)).toBeTruthy();
    expect(mock.pickInvoiceXml).toHaveBeenCalledTimes(1);
    await click("Cancelar revisión"); await click("Importar factura"); expect(mock.pickInvoiceXml).toHaveBeenCalledTimes(2);
  });
});

describe("detail and file operations", () => {
  it("separates detail errors from collection, retries and preserves the list on return", async () => {
    mock.getPurchaseInvoiceDetail.mockRejectedValueOnce(failed); await detail();
    expect(within(screen.getByRole("dialog")).getByText("Fallo controlado")).toBeTruthy();
    await click("Reintentar detalle"); expect(screen.getByText(/Registro en la app/)).toBeTruthy();
    await click("Cerrar"); expect(screen.getByText("26 facturas en resultados")).toBeTruthy();
  });
  it("attaches once, reflects returned detail before reload and never replaces an existing PDF", async () => {
    const pending = deferred<typeof invoiceDetail | null>(); mock.attachInvoicePdf.mockReturnValueOnce(pending.promise);
    await detail(); await click("Adjuntar PDF"); fireEvent.click(screen.getByRole("button", { name: "Adjuntar PDF" }));
    expect(mock.attachInvoicePdf).toHaveBeenCalledTimes(1); expect(mock.attachInvoicePdf).toHaveBeenCalledWith(7);
    mock.getPurchaseInvoicesView.mockRejectedValue(failed);
    await act(async () => pending.resolve({ ...invoiceDetail, pdf: invoicePdf }));
    expect(screen.getByText(/PDF adjuntado por el usuario: usuario.pdf/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Adjuntar PDF" })).toBeNull();
    await click("Exportar PDF"); await click("Exportar XML"); await click("Abrir PDF");
    expect(mock.exportInvoiceAttachment.mock.calls).toEqual([[7, "pdf"], [7, "xml"]]); expect(mock.openInvoicePdf).toHaveBeenCalledWith(7);
    expect(within(screen.getByRole("dialog")).queryByRole("alert")).toBeNull();
    await tick(); expect(screen.getByText(/El documento sigue registrado/)).toBeTruthy();
  });
  it("cancelling PDF attach leaves detail unchanged", async () => { await detail(); await click("Adjuntar PDF"); expect(screen.getByRole("button", { name: "Adjuntar PDF" })).toBeTruthy(); expect(mock.getPurchaseInvoicesView).toHaveBeenCalledTimes(1); });
});

describe("authentication and lifetimes", () => {
  it("forwards current post-registration cleanup authentication failure without undoing confirmed detail", async () => {
    const pending = deferred<void>(); mock.discardInvoicePreview.mockReturnValueOnce(pending.promise);
    const { auth } = await start(); await click("Importar factura"); await click("Registrar factura");
    expect(screen.getByRole("dialog", { name: "Detalle de factura" })).toBeTruthy();
    await act(async () => pending.reject(sessionError));
    expect(auth).toHaveBeenCalledOnce();
    expect(screen.getByText(/Factura registrada/)).toBeTruthy();
    expect(screen.queryByRole("dialog", { name: "Revisar factura" })).toBeNull();
    expect(mock.registerInvoice).toHaveBeenCalledOnce();
  });
  it("ignores post-registration cleanup authentication failure from an old lifetime", async () => {
    const pending = deferred<void>(); mock.discardInvoicePreview.mockReturnValueOnce(pending.promise);
    const { rerender, auth } = await start(); await click("Importar factura"); await click("Registrar factura");
    rerender(<PurchasesView sessionId={2} onSessionRequired={auth} />);
    await act(async () => pending.reject(sessionError));
    expect(auth).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).toBeNull();
  });
  it.each(["list", "suppliers", "detail", "xml", "previewPdf", "discard", "register", "attach", "exportXml", "exportPdf", "open"])("forwards current authentication error from %s", async (operation) => {
    const auth = vi.fn();
    if (operation === "list") mock.getPurchaseInvoicesView.mockRejectedValue(sessionError);
    if (operation === "suppliers") mock.searchInvoiceSuppliers.mockRejectedValue(sessionError);
    if (operation === "detail") mock.getPurchaseInvoiceDetail.mockRejectedValue(sessionError);
    render(<PurchasesView sessionId={1} onSessionRequired={auth} />); await tick();
    if (["detail", "attach", "exportXml", "exportPdf", "open"].includes(operation)) {
      if (["exportPdf", "open"].includes(operation)) mock.getPurchaseInvoiceDetail.mockResolvedValue({ ...invoiceDetail, pdf: invoicePdf });
      await click("Ver detalle");
      if (operation === "attach") { mock.attachInvoicePdf.mockRejectedValue(sessionError); await click("Adjuntar PDF"); }
      if (operation === "exportXml" || operation === "exportPdf") { mock.exportInvoiceAttachment.mockRejectedValue(sessionError); await click(operation === "exportXml" ? "Exportar XML" : "Exportar PDF"); }
      if (operation === "open") { mock.openInvoicePdf.mockRejectedValue(sessionError); await click("Abrir PDF"); }
    } else if (!["list", "suppliers"].includes(operation)) {
      if (operation === "xml") mock.pickInvoiceXml.mockRejectedValue(sessionError);
      await click("Importar factura");
      if (operation === "previewPdf") { mock.pickInvoicePreviewPdf.mockRejectedValue(sessionError); await click("Adjuntar PDF a revisión"); }
      if (operation === "discard") { mock.discardInvoicePreview.mockRejectedValue(sessionError); await click("Cancelar revisión"); }
      if (operation === "register") { mock.registerInvoice.mockRejectedValue(sessionError); await click("Registrar factura"); }
    }
    expect(auth).toHaveBeenCalledOnce();
  });
  it.each(["unmount", "session"])("cleans a long native picker on %s without reopening or writing obsolete UI", async (end) => {
    const pending = deferred<typeof invoicePreview | null>(); mock.pickInvoiceXml.mockReturnValueOnce(pending.promise);
    const { unmount, rerender, auth } = await start(); await click("Importar factura");
    if (end === "unmount") unmount(); else rerender(<PurchasesView sessionId={2} onSessionRequired={auth} />);
    await act(async () => pending.resolve(invoicePreview));
    expect(mock.discardInvoicePreview).toHaveBeenCalledWith(invoicePreview.review_token);
    expect(screen.queryByRole("dialog")).toBeNull(); expect(mock.registerInvoice).not.toHaveBeenCalled();
  });
  it("ignores obsolete detail authentication errors after session changes", async () => {
    const pending = deferred<typeof invoiceDetail>(); mock.getPurchaseInvoiceDetail.mockReturnValueOnce(pending.promise);
    const { rerender, auth } = await start(); await click("Ver detalle");
    rerender(<PurchasesView sessionId={2} onSessionRequired={auth} />);
    await act(async () => pending.reject(sessionError)); expect(auth).not.toHaveBeenCalled(); expect(screen.queryByRole("dialog")).toBeNull();
  });
  it("ignores an obsolete registration failure after session changes", async () => {
    const pending = deferred<Awaited<ReturnType<typeof api.registerInvoice>>>(); mock.registerInvoice.mockReturnValueOnce(pending.promise);
    const { rerender, auth } = await start(); await click("Importar factura"); await click("Registrar factura");
    rerender(<PurchasesView sessionId={2} onSessionRequired={auth} />);
    await act(async () => pending.reject(sessionError)); expect(auth).not.toHaveBeenCalled(); expect(screen.queryByRole("dialog")).toBeNull();
    expect(mock.discardInvoicePreview).toHaveBeenCalledWith(invoicePreview.review_token);
  });
  it("cleans a PDF picker result arriving after navigation and does not reopen review", async () => {
    const pending = deferred<typeof invoicePreview | null>(); mock.pickInvoicePreviewPdf.mockReturnValueOnce(pending.promise);
    const { unmount, auth } = await start(); await click("Importar factura"); await click("Adjuntar PDF a revisión"); unmount();
    await act(async () => pending.resolve({ ...invoicePreview, pdf: invoicePdf }));
    expect(mock.discardInvoicePreview).toHaveBeenCalledWith(invoicePreview.review_token);
    expect(screen.queryByRole("dialog")).toBeNull(); expect(auth).not.toHaveBeenCalled(); expect(mock.registerInvoice).not.toHaveBeenCalled();
  });
});
