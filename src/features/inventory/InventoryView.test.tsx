import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPurchase, createStockCount, getInventoryView, getProductInventoryDetail, lookupInventoryMovement, quotePurchase, quoteStockCount, searchInventoryProducts } from "../../api/inventory";
import { deferred, user } from "../../test/fixtures";
import type { InventoryMovement, InventoryMovementProduct, InventoryView as ViewData, PurchaseQuote, StockCountQuote } from "../../types/inventory_movement";
import { InventoryView } from "./InventoryView";
import { readPendingManualMovement, savePendingManualMovement } from "./manual-movement-intent";

vi.mock("../../api/inventory", () => ({
  createPurchase: vi.fn(), createStockCount: vi.fn(), getInventoryView: vi.fn(), lookupInventoryMovement: vi.fn(),
  quotePurchase: vi.fn(), quoteStockCount: vi.fn(), searchInventoryProducts: vi.fn(), getProductInventoryDetail: vi.fn(),
  quotePurchasePriceCorrection: vi.fn(), confirmPurchasePriceCorrection: vi.fn(),
}));

const presentation = { id: 101, product_id: 11, name: "Metro", mode: "measured" as const, base_quantity: 100, is_active: true };
const product: InventoryMovementProduct = { id: 11, name: "Cable", sku: "CABLE", stock: 175, base_unit: "centimeter", presentations: [presentation] };
const movement: InventoryMovement = {
  id: 1, product_id: 11, product_name: "Cable", sku: "CABLE", product_name_snapshot: "Cable historico", sku_snapshot: "CABLE-OLD", sale_id: null, ticket_reference: null,
  record_version: 3, origin: "manual", presentation_id: 101, presentation_name: "Metro", presentation_barcode: null, base_unit: "centimeter", presentation_mode: "measured", presentation_base_quantity: 100,
  purchase_unit_price_cents: 2500, purchase_total_cents: 1875, reason: "compra", type: "entrada", quantity: 75, entered_quantity: 75, base_quantity: 75, stock_before: 100, stock_after: 175,
  supplier: "Proveedor", document_reference: "FAC-1", note: "Lote", operation_token: "token", created_at: "2026-10-01T10:00:00Z", updated_at: "2026-10-01T10:00:00Z",
};
const view: ViewData = { items: [movement], products: [product], movements: [movement], product_page: 1, product_total: 1, movement_page: 1, movement_total: 1, page_size: 20, stats: { product_count: 1, piece_stock: 0, gram_stock: 0, milliliter_stock: 0, centimeter_stock: 175 } };
const purchaseQuote: PurchaseQuote = { product_id: 11, product_name: "Cable", product_sku: "CABLE", presentation_id: 101, presentation_name: "Metro", capture_name: "Metro", presentation_mode: "measured", presentation_base_quantity: 100, entered_quantity: 75, base_quantity: 75, stock_before: 175, stock_after: 250, purchase_unit_price_cents: 2500, purchase_total_cents: 1875, purchase_price_description: "por 1 m", base_unit: "centimeter", supplier: "Proveedor", document_reference: "FAC-1", note: null, quote_fingerprint: "purchase:v1" };
const countQuote: StockCountQuote = { product_id: 11, product_name: "Cable", product_sku: "CABLE", base_unit: "centimeter", stock_before: 175, counted_stock: 150, difference: -25, type: "salida", base_quantity: 25, adjustment_reason: "Merma", quote_fingerprint: "count:v1" };

const mocks = { view: vi.mocked(getInventoryView), search: vi.mocked(searchInventoryProducts), quotePurchase: vi.mocked(quotePurchase), createPurchase: vi.mocked(createPurchase), quoteCount: vi.mocked(quoteStockCount), createCount: vi.mocked(createStockCount), lookup: vi.mocked(lookupInventoryMovement) };
async function settle() { await act(() => vi.advanceTimersByTimeAsync(250)); await act(async () => undefined); }
function renderView(onNotice = vi.fn(), onSessionRequired = vi.fn()) { return render(<InventoryView user={user} onNotice={onNotice} onSessionRequired={onSessionRequired} />); }

beforeEach(() => {
  vi.useFakeTimers(); localStorage.clear();
  mocks.view.mockReset().mockResolvedValue(view); mocks.search.mockReset().mockResolvedValue({ items: [product], page: 1, total: 1, page_size: 20 });
  mocks.quotePurchase.mockReset().mockResolvedValue(purchaseQuote); mocks.createPurchase.mockReset().mockResolvedValue(movement);
  mocks.quoteCount.mockReset().mockResolvedValue(countQuote); mocks.createCount.mockReset().mockResolvedValue({ ...movement, reason: "ajuste", type: "salida", presentation_id: null, presentation_name: null, presentation_mode: null, presentation_base_quantity: null, purchase_unit_price_cents: null, purchase_total_cents: null, note: "Merma" });
  mocks.lookup.mockReset().mockResolvedValue(null);
  vi.mocked(getProductInventoryDetail).mockReset().mockResolvedValue({ product, movements: [movement], last_movement_at: movement.created_at, page: 1, total: 1, page_size: 20 });
});

async function prepareAction(kind: "purchase" | "stock_count") {
  fireEvent.click(screen.getByRole("button", { name: kind === "purchase" ? "Comprar" : "Contar" }));
  if (kind === "purchase") {
    fireEvent.click(screen.getByLabelText(/Metro/));
    fireEvent.change(screen.getByLabelText(/Cantidad en centimetros/), { target: { value: "75" } });
    fireEvent.change(screen.getByLabelText(/Precio de compra/), { target: { value: "25.00" } });
    fireEvent.change(screen.getByLabelText(/Proveedor/), { target: { value: "Proveedor" } });
    fireEvent.change(screen.getByLabelText(/Factura o referencia/), { target: { value: "FAC-1" } });
    fireEvent.change(screen.getByLabelText(/Nota opcional/), { target: { value: "Lote" } });
  } else {
    fireEvent.change(screen.getByLabelText(/Stock contado/), { target: { value: "150" } });
    fireEvent.change(screen.getByLabelText(/Motivo del ajuste/), { target: { value: "Merma" } });
  }
  fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
}

describe("InventoryView", () => {
  const sessionError = { category: "authentication", code: "SESSION_REQUIRED", message: "Sesion vencida" };

  describe.each(["purchase", "stock_count"] as const)("initial %s quote session isolation", (kind) => {
    it.each(["success", "session", "field"] as const)("ignores an old %s completion across A->B->A without unlocking the new quote", async (outcome) => {
      const old = deferred<PurchaseQuote & StockCountQuote>();
      const current = deferred<PurchaseQuote & StockCountQuote>();
      const quoteMock = kind === "purchase" ? mocks.quotePurchase : mocks.quoteCount;
      quoteMock.mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise);
      const notice = vi.fn(); const sessionRequired = vi.fn();
      const rendered = renderView(notice, sessionRequired); await settle(); await prepareAction(kind);
      rendered.rerender(<InventoryView user={{ ...user, id: user.id + 1 }} onNotice={notice} onSessionRequired={sessionRequired} />);
      rendered.rerender(<InventoryView user={user} onNotice={notice} onSessionRequired={sessionRequired} />);
      await settle(); await prepareAction(kind);
      await act(async () => {
        if (outcome === "success") old.resolve({ ...purchaseQuote, ...countQuote });
        else old.reject(outcome === "session" ? sessionError : { category: "validation", code: "REQUIRED", field: "quantity", message: "Error obsoleto" });
      });
      expect(screen.queryByText(/Resumen de compra|Resumen del conteo|Error obsoleto/)).toBeNull();
      expect((screen.getByRole("button", { name: "Calcular resumen" }) as HTMLButtonElement).disabled).toBe(true);
      expect(notice).not.toHaveBeenCalled(); expect(sessionRequired).not.toHaveBeenCalled();
      await act(async () => current.resolve({ ...purchaseQuote, ...countQuote }));
      expect((screen.getByRole("button", { name: kind === "purchase" ? "Confirmar compra" : "Confirmar ajuste" }) as HTMLButtonElement).disabled).toBe(false);
    });

    it.each(["success", "session", "field"] as const)("ignores a %s completion after unmount", async (outcome) => {
      const response = deferred<PurchaseQuote & StockCountQuote>();
      (kind === "purchase" ? mocks.quotePurchase : mocks.quoteCount).mockReturnValueOnce(response.promise);
      const notice = vi.fn(); const sessionRequired = vi.fn();
      const rendered = renderView(notice, sessionRequired); await settle(); await prepareAction(kind);
      rendered.unmount();
      const focus = vi.spyOn(HTMLElement.prototype, "focus");
      await act(async () => {
        if (outcome === "success") response.resolve({ ...purchaseQuote, ...countQuote });
        else response.reject(outcome === "session" ? sessionError : { category: "validation", code: "REQUIRED", field: "quantity", message: "Error obsoleto" });
      });
      await act(() => vi.advanceTimersByTimeAsync(16));
      expect(focus).not.toHaveBeenCalled(); focus.mockRestore();
      expect(notice).not.toHaveBeenCalled(); expect(sessionRequired).not.toHaveBeenCalled();
      expect(mocks.createPurchase).not.toHaveBeenCalled(); expect(mocks.createCount).not.toHaveBeenCalled();
    });
  });

  describe.each(["load", "selector", "recovery-search", "confirm"] as const)("%s async session isolation", (stage) => {
    it.each(["switch-success", "switch-session", "unmount-success", "unmount-session"] as const)("ignores %s completions", async (scenario) => {
      const response = deferred<unknown>();
      if (stage === "load") mocks.view.mockReturnValueOnce(response.promise.then((result) => result as ViewData));
      if (stage === "selector" || stage === "recovery-search") mocks.search.mockReturnValueOnce(response.promise.then((result) => result as Awaited<ReturnType<typeof searchInventoryProducts>>));
      if (stage === "confirm") mocks.createCount.mockReturnValueOnce(response.promise.then((result) => result as InventoryMovement));
      const legacy = { version: 1, userId: user.id, operationToken: "legacy-isolation", productId: 11, reason: "compra", type: "entrada", quantity: "1" } as const;
      if (stage === "recovery-search") savePendingManualMovement(legacy);
      const notice = vi.fn(); const sessionRequired = vi.fn();
      const rendered = renderView(notice, sessionRequired); await settle();
      if (stage === "selector") {
        fireEvent.click(screen.getByRole("button", { name: "Registrar compra" })); await settle();
      } else if (stage === "recovery-search") {
        fireEvent.click(screen.getByRole("button", { name: "Revisar operacion" })); await act(async () => undefined);
      } else if (stage === "confirm") {
        await prepareAction("stock_count");
        fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
      }
      const original = readPendingManualMovement(user.id);
      const loads = mocks.view.mock.calls.length;
      if (scenario.startsWith("unmount")) rendered.unmount();
      else {
        rendered.rerender(<InventoryView user={{ ...user, id: user.id + 1 }} onNotice={notice} onSessionRequired={sessionRequired} />);
        rendered.rerender(<InventoryView user={user} onNotice={notice} onSessionRequired={sessionRequired} />);
      }
      await act(async () => {
        if (scenario.endsWith("session")) response.reject(sessionError);
        else response.resolve(stage === "load" ? view : stage === "confirm" ? movement : { items: [product], page: 1, total: 1, page_size: 20 });
      });
      expect(notice).not.toHaveBeenCalled(); expect(sessionRequired).not.toHaveBeenCalled();
      expect(readPendingManualMovement(user.id)).toEqual(original);
      expect(mocks.view).toHaveBeenCalledTimes(loads);
      expect(screen.queryByRole("dialog")).toBeNull();
      if (scenario.startsWith("switch")) expect(screen.getByRole("tabpanel", { name: "Productos" }).getAttribute("aria-busy")).toBe("true");
    });
  });

  it.each(["purchase", "stock_count"] as const)("resolves an applied %s lost response after close and blocks duplication from detail", async (kind) => {
    const create = kind === "purchase" ? mocks.createPurchase : mocks.createCount;
    create.mockRejectedValueOnce(new Error("transport"));
    renderView(); await settle(); await prepareAction(kind);
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    const original = readPendingManualMovement(user.id)!;
    fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    fireEvent.click(screen.getByRole("button", { name: "Ver historial" })); await settle();
    for (const name of ["Registrar compra", "Ajustar existencias"]) {
      const button = screen.getByRole("button", { name }) as HTMLButtonElement;
      expect(button.disabled).toBe(true); fireEvent.click(button);
    }
    expect(screen.queryByRole("dialog")).toBeNull(); expect(readPendingManualMovement(user.id)).toEqual(original);
    fireEvent.click(screen.getByRole("button", { name: "Volver al inventario" }));
    mocks.lookup.mockResolvedValueOnce({ ...movement, operation_token: original.operationToken });
    fireEvent.click(screen.getByRole("button", { name: "Reintentar envio" })); await act(async () => undefined);
    expect(mocks.lookup).toHaveBeenCalledWith({ operation_token: original.operationToken });
    expect(create).toHaveBeenCalledTimes(1); expect(readPendingManualMovement(user.id)).toBeNull();
  });

  it.each(["purchase", "stock_count"] as const)("restores stale %s data and original token after closing and remounting", async (kind) => {
    const create = kind === "purchase" ? mocks.createPurchase : mocks.createCount;
    const stale = { category: "conflict", code: "QUOTE_CHANGED", message: "stale" };
    create.mockRejectedValueOnce(stale).mockRejectedValueOnce(stale);
    const first = renderView(); await settle(); await prepareAction(kind);
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    const original = readPendingManualMovement(user.id)!;
    fireEvent.click(screen.getByRole("button", { name: "Cancelar" })); first.unmount();
    renderView(); await settle(); fireEvent.click(screen.getByRole("button", { name: "Reintentar envio" })); await act(async () => undefined);
    expect(mocks.lookup).toHaveBeenCalledWith({ operation_token: original.operationToken });
    expect(screen.getByRole("alert").textContent).toContain("Cotiza nuevamente");
    if (kind === "purchase") {
      expect((screen.getByLabelText(/Metro/) as HTMLInputElement).checked).toBe(true);
      for (const [label, value] of [[/Cantidad en centimetros/, "75"], [/Precio de compra/, "25.00"], [/Proveedor/, "Proveedor"], [/Factura o referencia/, "FAC-1"], [/Nota opcional/, "Lote"]] as const) expect((screen.getByLabelText(label) as HTMLInputElement).value).toBe(value);
      mocks.quotePurchase.mockResolvedValueOnce({ ...purchaseQuote, quote_fingerprint: "purchase:v2" });
    } else {
      expect((screen.getByLabelText(/Stock contado/) as HTMLInputElement).value).toBe("150");
      expect((screen.getByLabelText(/Motivo del ajuste/) as HTMLInputElement).value).toBe("Merma");
      mocks.quoteCount.mockResolvedValueOnce({ ...countQuote, quote_fingerprint: "count:v2" });
    }
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    expect(create).toHaveBeenLastCalledWith(expect.objectContaining({ operation_token: original.operationToken, quote_fingerprint: kind === "purchase" ? "purchase:v2" : "count:v2" }));
    expect(readPendingManualMovement(user.id)).toBeNull();
  });

  it("keeps recovery errors visible with the modal closed", async () => {
    savePendingManualMovement({ version: 5, kind: "stock_count", userId: user.id, operationToken: "closed", productId: 11, countedStock: "150", adjustmentReason: "Merma", quoteFingerprint: "old" });
    mocks.lookup.mockRejectedValueOnce({ category: "conflict", code: "LOOKUP_FAILED", message: "Consulta fallida" });
    renderView(); await settle(); fireEvent.click(screen.getByRole("button", { name: "Reintentar envio" })); await act(async () => undefined);
    expect(screen.queryByRole("dialog")).toBeNull(); expect(screen.getByRole("alert").textContent).toContain("Consulta fallida");
    expect(mocks.createCount).not.toHaveBeenCalled(); expect(readPendingManualMovement(user.id)?.operationToken).toBe("closed");
  });

  it("blocks presentation changes during a slow quote and can quote again afterwards", async () => {
    const slow = deferred<PurchaseQuote>();
    renderView(); await settle(); await prepareAction("purchase");
    fireEvent.click(screen.getByLabelText(/Captura directa/));
    mocks.quotePurchase.mockReturnValueOnce(slow.promise);
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    const metro = screen.getByLabelText(/Metro/) as HTMLInputElement;
    expect(metro.closest("fieldset[disabled]")).not.toBeNull();
    metro.click(); expect(metro.checked).toBe(false);
    await act(async () => slow.resolve(purchaseQuote));
    fireEvent.click(metro); expect(screen.queryByText("Resumen de compra")).toBeNull();
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    expect(screen.getByText("Resumen de compra")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Confirmar compra" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("releases an invalidated quote without letting its completion unlock a newer request", async () => {
    const first = deferred<PurchaseQuote>(); const second = deferred<PurchaseQuote>();
    renderView(); await settle(); await prepareAction("purchase");
    fireEvent.change(screen.getByLabelText(/Proveedor/), { target: { value: "Otro" } });
    mocks.quotePurchase.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    // Synthetic change covers invalidation even if a queued event bypasses the disabled group.
    fireEvent.change(screen.getByLabelText(/Proveedor/), { target: { value: "Nuevo" } });
    expect((screen.getByRole("button", { name: "Calcular resumen" }) as HTMLButtonElement).disabled).toBe(false);
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    await act(async () => first.resolve(purchaseQuote));
    expect(screen.queryByText("Resumen de compra")).toBeNull();
    expect((screen.getByRole("button", { name: "Calcular resumen" }) as HTMLButtonElement).disabled).toBe(true);
    await act(async () => second.resolve({ ...purchaseQuote, supplier: "Nuevo" }));
    expect(screen.getByText("Resumen de compra")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Confirmar compra" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("guards opening actions against a pending token added after render", async () => {
    renderView(); await settle();
    const original = { version: 5, kind: "stock_count", userId: user.id, operationToken: "other-view", productId: 11, countedStock: "150", adjustmentReason: "Merma", quoteFingerprint: "old" } as const;
    savePendingManualMovement(original);
    fireEvent.click(screen.getByRole("button", { name: "Comprar" }));
    expect(screen.queryByRole("dialog")).toBeNull(); expect(readPendingManualMovement(user.id)).toEqual(original);
    expect(screen.getByText("Operacion pendiente")).toBeTruthy();
  });
  it("removes valuation and cost UI and exposes two separate actions", async () => {
    renderView(); await settle();
    expect(screen.getByRole("button", { name: "Registrar compra" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Ajustar existencias" })).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/costo promedio|valor a costo|costo desconocido|registrar movimiento|valoracion/i);
    expect(screen.getByText("Existencia por longitud")).toBeTruthy();
  });

  it("quotes and confirms a purchase with explicit price basis and a v5 intent", async () => {
    mocks.createPurchase.mockImplementation(async (input) => {
      const stored = JSON.parse(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)!);
      expect(stored).toEqual(expect.objectContaining({ version: 5, kind: "purchase", purchaseUnitPrice: "25.00", supplier: "Proveedor" }));
      expect(input.operation_token).toBe(stored.operationToken); return movement;
    });
    renderView(); await settle(); fireEvent.click(screen.getByRole("button", { name: "Comprar" }));
    fireEvent.click(screen.getByLabelText(/Metro/));
    fireEvent.change(screen.getByLabelText(/Cantidad en centimetros/), { target: { value: "75" } });
    fireEvent.change(screen.getByLabelText(/Precio de compra por 1 m/), { target: { value: "25.00" } });
    fireEvent.change(screen.getByLabelText(/Proveedor/), { target: { value: "Proveedor" } });
    fireEvent.change(screen.getByLabelText(/Factura o referencia/), { target: { value: "FAC-1" } });
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    expect(mocks.quotePurchase).toHaveBeenCalledWith({ product_id: 11, presentation_id: 101, quantity: "75", purchase_unit_price: "25.00", supplier: "Proveedor", document_reference: "FAC-1", note: null });
    expect(screen.getByText("Resumen de compra")).toBeTruthy(); expect(screen.getByText("Importe total:").parentElement?.textContent).toContain("$18.75");
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    expect(mocks.createPurchase).toHaveBeenCalledWith(expect.objectContaining({ purchase_unit_price: "25.00", quote_fingerprint: "purchase:v1" }));
  });

  it("keeps stock count free of commercial and presentation fields", async () => {
    renderView(); await settle(); fireEvent.click(screen.getByRole("button", { name: "Contar" }));
    expect(screen.getByText((_, node) => node?.textContent === "Unidad de conteo: centimetros. No uses presentaciones ni datos comerciales.")).toBeTruthy();
    expect(screen.queryByLabelText(/Proveedor|Factura|Precio de compra|Forma o presentacion/)).toBeNull();
    fireEvent.change(screen.getByLabelText(/Stock contado en centimetros/), { target: { value: "150" } });
    fireEvent.change(screen.getByLabelText(/Motivo del ajuste/), { target: { value: "Merma" } });
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    expect(mocks.quoteCount).toHaveBeenCalledWith({ product_id: 11, counted_stock: "150", adjustment_reason: "Merma" });
    expect(screen.getByText("Resumen del conteo")).toBeTruthy(); expect(screen.getByText("Resultado:").parentElement?.textContent).toContain("Salida");
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    expect(mocks.createCount).toHaveBeenCalledWith(expect.objectContaining({ counted_stock: "150", adjustment_reason: "Merma", quote_fingerprint: "count:v1" }));
  });

  it("invalidates a quote when purchase metadata changes", async () => {
    renderView(); await settle(); fireEvent.click(screen.getByRole("button", { name: "Comprar" }));
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    expect(screen.getByText("Resumen de compra")).toBeTruthy();
    fireEvent.change(screen.getByLabelText(/Proveedor/), { target: { value: "Otro" } });
    expect(screen.queryByText("Resumen de compra")).toBeNull(); expect(mocks.createPurchase).not.toHaveBeenCalled();
  });

  it("renders purchase, adjustment and sale as responsive cards", async () => {
    mocks.view.mockResolvedValueOnce({ ...view, movements: [movement, { ...movement, id: 2, reason: "ajuste", type: "salida", purchase_unit_price_cents: null, purchase_total_cents: null, presentation_id: null, presentation_name: null, note: "Merma" }, { ...movement, id: 3, reason: "venta", type: "salida", origin: "sale", purchase_unit_price_cents: null, purchase_total_cents: null, ticket_reference: "T-9" }] });
    renderView(); await settle(); fireEvent.click(screen.getByRole("tab", { name: "Movimientos" }));
    const panel = screen.getByRole("tabpanel", { name: "Movimientos" });
    expect(within(panel).getByText("Compra · Entrada")).toBeTruthy(); expect(within(panel).getByText("Ajuste · Salida")).toBeTruthy(); expect(within(panel).getByText("Venta · Salida")).toBeTruthy();
  });

  it("consults a legacy token and demands recapture when it was not applied", async () => {
    savePendingManualMovement({ version: 2, userId: user.id, operationToken: "legacy", productId: 11, presentationId: 101, reason: "compra", type: "entrada", quantity: "75", unitPrice: "25", supplier: "Viejo", documentReference: "OLD", note: null, quoteFingerprint: "old" });
    renderView(); await settle(); fireEvent.click(screen.getByRole("button", { name: "Revisar operacion" })); await act(async () => undefined);
    expect(mocks.lookup).toHaveBeenCalledWith({ operation_token: "legacy" }); expect(mocks.quotePurchase).not.toHaveBeenCalled();
    expect(screen.getByRole("alert").textContent).toContain("Recaptura"); expect((screen.getByLabelText(/Precio de compra/) as HTMLInputElement).value).toBe("");
  });

  it("clears a legacy intent already applied without opening a form", async () => {
    const notice = vi.fn(); mocks.lookup.mockResolvedValueOnce(movement);
    savePendingManualMovement({ version: 1, userId: user.id, operationToken: "applied", productId: 11, reason: "compra", type: "entrada", quantity: "1" });
    renderView(notice); await settle(); fireEvent.click(screen.getByRole("button", { name: "Revisar operacion" })); await act(async () => undefined);
    expect(screen.queryByRole("dialog")).toBeNull(); expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).toBeNull(); expect(notice).toHaveBeenCalledWith(expect.objectContaining({ message: expect.stringContaining("no se duplico") }));
  });

  it("shows backend field errors and requires a fresh quote after stale confirmation", async () => {
    mocks.quoteCount.mockRejectedValueOnce({ category: "validation", code: "REQUIRED", field: "adjustment_reason", message: "Captura el motivo" });
    renderView(); await settle(); fireEvent.click(screen.getByRole("button", { name: "Contar" })); fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    expect(screen.getByText("Captura el motivo")).toBeTruthy();
    mocks.quoteCount.mockResolvedValueOnce(countQuote); fireEvent.change(screen.getByLabelText(/Motivo del ajuste/), { target: { value: "Merma" } }); fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    mocks.createCount.mockRejectedValueOnce({ category: "conflict", code: "QUOTE_CHANGED", message: "stale" }); fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    expect(screen.getByRole("alert").textContent).toContain("Cotiza nuevamente"); expect(screen.getByRole("button", { name: "Calcular resumen" })).toBeTruthy();
  });

  it.each(["purchase", "stock_count"] as const)("reuses the original token when an applied %s response is lost", async (kind) => {
    const tokens: string[] = [];
    const onNotice = vi.fn();
    if (kind === "purchase") {
      mocks.createPurchase
        .mockImplementationOnce(async (input) => { tokens.push(input.operation_token); throw new Error("transport"); })
        .mockImplementationOnce(async (input) => { tokens.push(input.operation_token); return movement; });
    } else {
      mocks.createCount
        .mockImplementationOnce(async (input) => { tokens.push(input.operation_token); throw new Error("transport"); })
        .mockImplementationOnce(async (input) => { tokens.push(input.operation_token); return { ...movement, reason: "ajuste", type: "salida" }; });
    }

    renderView(onNotice); await settle();
    fireEvent.click(screen.getByRole("button", { name: kind === "purchase" ? "Comprar" : "Contar" }));
    if (kind === "purchase") {
      fireEvent.change(screen.getByLabelText(/Cantidad en centimetros/), { target: { value: "75" } });
      fireEvent.change(screen.getByLabelText(/Precio de compra/), { target: { value: "25" } });
      fireEvent.change(screen.getByLabelText(/Proveedor/), { target: { value: "Proveedor" } });
      fireEvent.change(screen.getByLabelText(/Factura o referencia/), { target: { value: "FAC-1" } });
    } else {
      fireEvent.change(screen.getByLabelText(/Stock contado/), { target: { value: "150" } });
      fireEvent.change(screen.getByLabelText(/Motivo del ajuste/), { target: { value: "Merma" } });
    }
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    const storedToken = JSON.parse(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)!).operationToken;
    expect(screen.getByRole("alert").textContent).toContain("reintentar");
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    expect(tokens).toEqual([storedToken, storedToken]); expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).toBeNull();
    expect(onNotice).toHaveBeenCalledWith(expect.objectContaining({ tone: "success" }));
  });

  it("ignores out-of-order selector responses and uses native list semantics", async () => {
    const first = deferred<Awaited<ReturnType<typeof searchInventoryProducts>>>();
    const second = deferred<Awaited<ReturnType<typeof searchInventoryProducts>>>();
    mocks.search.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    renderView(); await settle(); fireEvent.click(screen.getByRole("button", { name: "Registrar compra" }));
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.change(within(screen.getByRole("dialog")).getByLabelText("Buscar producto"), { target: { value: "nuevo" } }); await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => second.resolve({ items: [{ ...product, id: 12, name: "Resultado nuevo" }], page: 1, total: 1, page_size: 20 }));
    expect(screen.getByRole("list", { name: "Productos" })).toBeTruthy(); expect(screen.getByRole("button", { name: /Resultado nuevo/ })).toBeTruthy(); expect(screen.queryByRole("option")).toBeNull();
    await act(async () => first.resolve({ items: [{ ...product, name: "Resultado viejo" }], page: 1, total: 1, page_size: 20 }));
    expect(screen.queryByText("Resultado viejo")).toBeNull();
  });

  it("ignores a selector response after closing and propagates SESSION_REQUIRED", async () => {
    const pendingSearch = deferred<Awaited<ReturnType<typeof searchInventoryProducts>>>();
    const onSessionRequired = vi.fn();
    mocks.search.mockReturnValueOnce(pendingSearch.promise).mockRejectedValueOnce({ category: "authentication", code: "SESSION_REQUIRED", message: "Sesion vencida" });
    renderView(vi.fn(), onSessionRequired); await settle(); fireEvent.click(screen.getByRole("button", { name: "Registrar compra" })); await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Cancelar" })); await act(async () => pendingSearch.resolve({ items: [{ ...product, name: "Respuesta tardia" }], page: 1, total: 1, page_size: 20 }));
    expect(screen.queryByRole("dialog")).toBeNull(); expect(screen.queryByText("Respuesta tardia")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Registrar compra" })); await settle();
    expect(onSessionRequired).toHaveBeenCalledOnce(); expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("marks retained inventory stale and hides pagination during reload and error", async () => {
    const refresh = deferred<ViewData>();
    mocks.view.mockResolvedValueOnce({ ...view, product_total: 21 }).mockReturnValueOnce(refresh.promise);
    renderView(); await settle(); fireEvent.click(screen.getByRole("button", { name: "Siguiente" })); await act(() => vi.advanceTimersByTimeAsync(250));
    const productsPanel = screen.getByRole("tabpanel", { name: "Productos" });
    expect(productsPanel.getAttribute("aria-busy")).toBe("true"); expect(screen.getByRole("status").textContent).toContain("resultados anteriores"); expect(screen.queryByRole("navigation", { name: "Paginacion de productos" })).toBeNull();
    await act(async () => refresh.reject(new Error("transport")));
    expect(screen.getByText(/no corresponden a los filtros actuales/)).toBeTruthy(); expect(screen.queryByRole("navigation", { name: "Paginacion de productos" })).toBeNull();
  });

  it("marks filtered results stale during debounce and never requests the old page", async () => {
    mocks.view.mockResolvedValue({ ...view, product_total: 21 });
    renderView(); await settle(); fireEvent.click(screen.getByRole("button", { name: "Siguiente" })); await settle();
    expect(mocks.view).toHaveBeenLastCalledWith(expect.objectContaining({ product_page: 2 }));
    const calls = mocks.view.mock.calls.length;
    fireEvent.change(screen.getByLabelText("Buscar producto"), { target: { value: "cable" } });
    expect(screen.getByRole("tabpanel", { name: "Productos" }).getAttribute("aria-busy")).toBe("true");
    expect(screen.queryByRole("navigation", { name: "Paginacion de productos" })).toBeNull();
    await act(() => vi.advanceTimersByTimeAsync(249)); expect(mocks.view).toHaveBeenCalledTimes(calls);
    await act(() => vi.advanceTimersByTimeAsync(1)); await act(async () => undefined);
    expect(mocks.view).toHaveBeenLastCalledWith(expect.objectContaining({ product_search: "cable", product_page: 1 }));
    expect(mocks.view.mock.calls.slice(calls).some(([input]) => input?.product_search === "cable" && input.product_page !== 1)).toBe(false);
  });

  it("restores focus to the history trigger when returning from detail", async () => {
    renderView(); await settle();
    const trigger = screen.getByRole("button", { name: "Ver historial" }); trigger.focus(); fireEvent.click(trigger);
    fireEvent.click(screen.getByRole("button", { name: "Volver al inventario" })); await act(() => vi.advanceTimersByTimeAsync(16));
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Ver historial" }));
  });

  it.each(["purchase", "stock_count"] as const)("ignores deferred %s recovery lookups after unmount", async (kind) => {
    for (const outcome of ["missing", "applied", "session"] as const) {
      localStorage.clear();
      const intent = kind === "purchase"
        ? { version: 5, kind, userId: user.id, operationToken: `unmount-${kind}-${outcome}`, productId: 11, presentationId: 101, quantity: "75", purchaseUnitPrice: "25.00", supplier: "Proveedor", documentReference: "FAC-1", note: null, quoteFingerprint: "old" } as const
        : { version: 5, kind, userId: user.id, operationToken: `unmount-${kind}-${outcome}`, productId: 11, countedStock: "150", adjustmentReason: "Merma", quoteFingerprint: "old" } as const;
      savePendingManualMovement(intent);
      const lookup = deferred<InventoryMovement | null>();
      mocks.lookup.mockReturnValueOnce(lookup.promise);
      const notice = vi.fn(); const sessionRequired = vi.fn();
      const rendered = renderView(notice, sessionRequired); await settle();
      fireEvent.click(screen.getByRole("button", { name: "Reintentar envio" }));
      expect(mocks.lookup).toHaveBeenLastCalledWith({ operation_token: intent.operationToken });
      rendered.unmount();
      await act(async () => {
        if (outcome === "session") lookup.reject({ category: "authentication", code: "SESSION_REQUIRED", message: "Sesion vencida" });
        else lookup.resolve(outcome === "applied" ? movement : null);
      });
      expect(mocks.createPurchase).not.toHaveBeenCalled(); expect(mocks.createCount).not.toHaveBeenCalled();
      expect(mocks.search).not.toHaveBeenCalled(); expect(notice).not.toHaveBeenCalled(); expect(sessionRequired).not.toHaveBeenCalled();
      expect(readPendingManualMovement(user.id)).toEqual(intent);
    }
  });

  it.each(["missing", "applied", "session"] as const)("ignores an old user's %s lookup even after returning to that user", async (outcome) => {
    const original = { version: 5, kind: "stock_count", userId: user.id, operationToken: "old-user", productId: 11, countedStock: "150", adjustmentReason: "Merma", quoteFingerprint: "old" } as const;
    const otherUser = { ...user, id: user.id + 1 };
    const other = { ...original, userId: otherUser.id, operationToken: "new-user" };
    savePendingManualMovement(original); savePendingManualMovement(other);
    const oldLookup = deferred<InventoryMovement | null>(); const currentLookup = deferred<InventoryMovement | null>();
    mocks.lookup.mockReturnValueOnce(oldLookup.promise).mockReturnValueOnce(currentLookup.promise);
    const notice = vi.fn(); const sessionRequired = vi.fn();
    const rendered = renderView(notice, sessionRequired); await settle();
    fireEvent.click(screen.getByRole("button", { name: "Reintentar envio" }));
    rendered.rerender(<InventoryView user={otherUser} onNotice={notice} onSessionRequired={sessionRequired} />);
    expect((screen.getByRole("button", { name: "Reintentar envio" }) as HTMLButtonElement).disabled).toBe(false);
    rendered.rerender(<InventoryView user={user} onNotice={notice} onSessionRequired={sessionRequired} />);
    fireEvent.click(screen.getByRole("button", { name: "Reintentar envio" }));
    expect(mocks.lookup).toHaveBeenCalledTimes(2);
    await act(async () => {
      if (outcome === "session") oldLookup.reject({ category: "authentication", code: "SESSION_REQUIRED", message: "Sesion vencida" });
      else oldLookup.resolve(outcome === "applied" ? movement : null);
    });
    expect(mocks.createPurchase).not.toHaveBeenCalled(); expect(mocks.createCount).not.toHaveBeenCalled();
    expect(notice).not.toHaveBeenCalled(); expect(sessionRequired).not.toHaveBeenCalled();
    expect(readPendingManualMovement(user.id)).toEqual(original); expect(readPendingManualMovement(otherUser.id)).toEqual(other);
    expect((screen.getByRole("button", { name: "Reintentar envio" }) as HTMLButtonElement).disabled).toBe(true);
    await act(async () => currentLookup.resolve(null));
    expect(mocks.createCount).toHaveBeenCalledTimes(1); expect(readPendingManualMovement(user.id)).toBeNull();
    expect(notice).toHaveBeenCalledOnce();
  });

  it.each(["purchase", "stock_count"] as const)("ignores an in-flight %s confirmation after a user change", async (kind) => {
    const create = kind === "purchase" ? mocks.createPurchase : mocks.createCount;
    const response = deferred<InventoryMovement>(); create.mockReturnValueOnce(response.promise);
    const notice = vi.fn(); const sessionRequired = vi.fn();
    const rendered = renderView(notice, sessionRequired); await settle(); await prepareAction(kind);
    fireEvent.submit(document.getElementById("inventory-action-form")!); await act(async () => undefined);
    const original = readPendingManualMovement(user.id);
    rendered.rerender(<InventoryView user={{ ...user, id: user.id + 1 }} onNotice={notice} onSessionRequired={sessionRequired} />);
    await act(async () => response.resolve(movement));
    expect(readPendingManualMovement(user.id)).toEqual(original);
    expect(notice).not.toHaveBeenCalled(); expect(sessionRequired).not.toHaveBeenCalled();
  });
});
