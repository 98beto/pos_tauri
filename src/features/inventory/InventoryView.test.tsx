import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createInventoryMovement, getInventoryView, getProductInventoryDetail, quoteInventoryMovement, searchInventoryProducts } from "../../api/inventory";
import { deferred, user } from "../../test/fixtures";
import type { InventoryMovement, InventoryMovementQuote, InventoryView as InventoryViewData, ProductInventoryDetail } from "../../types/inventory_movement";
import { InventoryView } from "./InventoryView";
import { savePendingManualMovement } from "./manual-movement-intent";

vi.mock("../../api/inventory", () => ({ createInventoryMovement: vi.fn(), getInventoryView: vi.fn(), getProductInventoryDetail: vi.fn(), quoteInventoryMovement: vi.fn(), searchInventoryProducts: vi.fn() }));

const movement: InventoryMovement = {
  id: 1, product_id: 11, product_name: "Cafe molido", sku: "CAFE-1", product_name_snapshot: "Cafe anterior", sku_snapshot: "CAFE-1",
  sale_id: null, ticket_reference: null, record_version: 2, presentation_id: 101, presentation_name: "Bolsa 250 g", presentation_barcode: null,
  base_unit: "gram", presentation_mode: "fixed", presentation_base_quantity: 250, unit_price_cents: 1250, reason: "compra", type: "entrada",
  quantity: 500, entered_quantity: 2, base_quantity: 500, stock_before: 750, stock_after: 1250, movement_cost_cents: 2500,
  inventory_cost_before_cents: 3000, inventory_cost_after_cents: 5500, supplier: "Proveedor", document_reference: "FAC-1", note: "Lote", operation_token: "token", created_at: "2026-01-01T10:00:00Z", updated_at: "2026-01-01T10:00:00Z",
};
const presentation = { id: 101, product_id: 11, name: "Bolsa 250 g", mode: "fixed" as const, base_quantity: 250, price: 1250, barcode: null, is_default: true, is_active: true, is_conversion_locked: false, created_at: "2026-01-01", updated_at: "2026-01-01" };
const row = { id: 11, name: "Cafe molido", sku: "CAFE-1", stock: 1250, base_unit: "gram" as const, inventory_cost_cents: 5500, inventory_cost_status: "known" as const, average_cost_cents: 4 };
const inventoryRow = { ...row, presentations: [presentation] };
const emptyRow = { id: 22, name: "Producto sin movimientos", sku: "NUEVO", stock: 0, base_unit: "piece" as const, inventory_cost_cents: 0, inventory_cost_status: "known" as const, average_cost_cents: null, presentations: [] };
const inventory: InventoryViewData = {
  stats: { product_count: 2, piece_stock: 7, gram_stock: 1250, milliliter_stock: 2000, inventory_value_cents: 7500, products_without_cost: 1 },
  products: [inventoryRow, emptyRow], movements: [movement], items: [movement], product_page: 1, product_total: 21, movement_page: 1, movement_total: 21, page_size: 20,
};
const detail: ProductInventoryDetail = { product: row, last_movement_at: movement.created_at, movements: [movement], page: 1, total: 1, page_size: 20 };

const mockedView = vi.mocked(getInventoryView);
const mockedCreate = vi.mocked(createInventoryMovement);
const mockedQuote = vi.mocked(quoteInventoryMovement);
const mockedDetail = vi.mocked(getProductInventoryDetail);
const mockedSearch = vi.mocked(searchInventoryProducts);

function quote(overrides: Partial<InventoryMovementQuote> = {}): InventoryMovementQuote {
  return {
    product_id: 11, product_name: "Cafe molido", product_sku: "CAFE-1", reason: "compra", type: "entrada",
    presentation_id: 101, presentation_name: "Bolsa 250 g", capture_name: "Bolsa 250 g", presentation_mode: "fixed",
    presentation_base_quantity: 250, entered_quantity: 2, entered_quantity_description: "2 x Bolsa 250 g",
    base_quantity: 500, base_quantity_description: "500 gramos", unit_price_cents: 1250,
    unit_price_description: "por Bolsa 250 g", total_cost_cents: 2500, base_unit: "gram",
    quote_fingerprint: "inventory-quote-v1:test", ...overrides,
  };
}

function renderInventory(props: Partial<React.ComponentProps<typeof InventoryView>> = {}) {
  return render(<InventoryView user={user} onNotice={vi.fn()} onSessionRequired={vi.fn()} {...props} />);
}
async function settleInitial() { await act(() => vi.advanceTimersByTimeAsync(250)); await act(async () => undefined); }
async function openMovement() { fireEvent.click(screen.getByRole("button", { name: "Registrar movimiento" })); await act(() => vi.advanceTimersByTimeAsync(250)); await act(async () => undefined); }
function selectProduct(name = "Cafe molido") { fireEvent.click(screen.getByRole("option", { name: new RegExp(name) })); }
async function quoteAndConfirm() { fireEvent.submit(document.getElementById("inventory-movement-form")!); await act(async () => undefined); fireEvent.submit(document.getElementById("inventory-movement-form")!); await act(async () => undefined); }

beforeEach(() => {
  vi.useFakeTimers();
  localStorage.clear();
  mockedSearch.mockReset();
  mockedView.mockResolvedValue(inventory);
  mockedSearch.mockResolvedValue({ items: inventory.products, page: 1, total: inventory.product_total, page_size: 20 });
  mockedQuote.mockResolvedValue(quote());
  mockedCreate.mockResolvedValue(movement);
  mockedDetail.mockResolvedValue(detail);
});

describe("InventoryView", () => {
  it("renders accessible exclusive tabs, formatted statistics and the remote product page", async () => {
    renderInventory();
    await settleInitial();
    expect(screen.getByRole("tab", { name: "Productos" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tabpanel", { name: "Productos" }).hasAttribute("hidden")).toBe(false);
    expect(document.getElementById("inventory-movements-panel")?.hasAttribute("hidden")).toBe(true);
    expect(screen.getAllByText("1.25 kg").length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "Ver inventario de Cafe molido" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Ver inventario de Producto sin movimientos" })).toBeTruthy();
    for (const name of ["Productos de inventario desplazables", "Movimientos de inventario desplazables"]) {
      const region = screen.getByLabelText(name);
      expect(region.getAttribute("role")).toBe("region");
      expect(region.className).toContain("focus-visible:outline-2");
    }
    expect(mockedView).toHaveBeenCalledWith(expect.objectContaining({ product_page: 1, movement_page: 1 }));
  });

  it("moves and activates tabs with the keyboard", async () => {
    renderInventory(); await settleInitial();
    const productsTab = screen.getByRole("tab", { name: "Productos" });
    productsTab.focus();
    fireEvent.keyDown(productsTab, { key: "ArrowRight" });
    await act(() => vi.runAllTimersAsync());
    const movementsTab = screen.getByRole("tab", { name: "Movimientos" });
    expect(movementsTab.getAttribute("aria-selected")).toBe("true");
    expect(movementsTab.getAttribute("tabindex")).toBe("0");
    expect(document.activeElement).toBe(movementsTab);
  });

  it("keeps independent tab filters and pages", async () => {
    renderInventory(); await settleInitial();
    fireEvent.change(screen.getByLabelText("Buscar producto"), { target: { value: "cafe" } });
    fireEvent.click(screen.getByRole("button", { name: "Siguiente" }));
    fireEvent.click(screen.getByRole("tab", { name: "Movimientos" }));
    fireEvent.change(screen.getByLabelText("Buscar movimiento"), { target: { value: "FAC" } });
    fireEvent.change(screen.getByLabelText("Motivo"), { target: { value: "compra" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    expect((screen.getByLabelText("Buscar producto") as HTMLInputElement).value).toBe("cafe");
    expect(mockedView).toHaveBeenLastCalledWith(expect.objectContaining({ product_search: "cafe", product_page: 2, movement_search: "FAC", movement_page: 1, reason: "compra" }));
  });

  it("opens products without movements from the remote catalog and returns with focus restored", async () => {
    mockedDetail.mockResolvedValueOnce({ ...detail, product: { ...row, id: 22, name: "Producto sin movimientos", sku: "NUEVO", stock: 0, base_unit: "piece" }, movements: [], last_movement_at: null, total: 0 });
    renderInventory(); await settleInitial();
    fireEvent.click(screen.getByRole("button", { name: "Ver inventario de Producto sin movimientos" }));
    await act(() => vi.advanceTimersByTimeAsync(250));
    expect(screen.getByRole("heading", { name: "Producto sin movimientos" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Volver al inventario" }));
    await act(() => vi.runAllTimersAsync());
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Ver inventario de Producto sin movimientos" }));
    expect((screen.getByLabelText("Buscar producto") as HTMLInputElement).value).toBe("");
  });

  it("restores focus to the matching row action after local detail navigation", async () => {
    renderInventory(); await settleInitial();
    const action = screen.getByRole("button", { name: "Ver inventario de Cafe molido" });
    fireEvent.click(action); await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Volver al inventario" }));
    await act(() => vi.runAllTimersAsync());
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Ver inventario de Cafe molido" }));
  });

  it.each([
    ["Compra", "purchase", { reason: "compra", type: "entrada", unit_price: "12.50" }],
    ["Ajuste de entrada", "adjustment-in", { reason: "ajuste", type: "entrada", unit_price: "12.50" }],
    ["Ajuste de salida", "adjustment-out", { reason: "ajuste", type: "salida", unit_price: null }],
  ] as const)("submits v2 metadata for %s", async (_label, operation, expected) => {
    renderInventory(); await settleInitial(); await openMovement();
    selectProduct();
    fireEvent.click(screen.getByRole("radio", { name: /Bolsa 250 g/ }));
    fireEvent.change(screen.getByLabelText("Operacion obligatorio"), { target: { value: operation } });
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "2" } });
    if (operation !== "adjustment-out") fireEvent.change(screen.getByLabelText(/^Precio de compra/), { target: { value: "12.50" } });
    fireEvent.change(screen.getByLabelText("Proveedor"), { target: { value: "Proveedor" } });
    fireEvent.change(screen.getByLabelText("Factura o referencia"), { target: { value: "FAC-1" } });
    fireEvent.change(screen.getByLabelText("Nota"), { target: { value: "Lote" } });
    await quoteAndConfirm();
    expect(mockedQuote).toHaveBeenCalledWith(expect.objectContaining({ product_id: 11, presentation_id: 101, quantity: "2", ...expected }));
    expect(mockedCreate).toHaveBeenCalledWith(expect.objectContaining({ product_id: 11, presentation_id: 101, quantity: "2", supplier: "Proveedor", document_reference: "FAC-1", note: "Lote", ...expected, operation_token: expect.any(String), quote_fingerprint: "inventory-quote-v1:test" }));
  });

  it("submits valuation with zero quantity, no presentation and the current total cost", async () => {
    renderInventory(); await settleInitial(); await openMovement();
    selectProduct();
    fireEvent.change(screen.getByLabelText("Operacion obligatorio"), { target: { value: "valuation" } });
    fireEvent.change(screen.getByLabelText(/^Nuevo valor total del inventario/), { target: { value: "50.00" } });
    await quoteAndConfirm();
    expect(mockedCreate).toHaveBeenCalledWith(expect.objectContaining({ presentation_id: null, reason: "valoracion", type: "valoracion", quantity: "0", unit_price: "50.00" }));
  });

  it("persists v2 before IPC, clears by token and reports the backend-confirmed total", async () => {
    const onNotice = vi.fn();
    mockedCreate.mockImplementation(async (input) => {
      const stored = JSON.parse(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)!);
      expect(stored.version).toBe(2); expect(stored.operationToken).toBe(input.operation_token);
      return movement;
    });
    renderInventory({ onNotice }); await settleInitial(); await openMovement();
    selectProduct();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "2" } });
    fireEvent.change(screen.getByLabelText(/^Precio de compra/), { target: { value: "12.50" } });
    await quoteAndConfirm();
    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).toBeNull();
    expect(onNotice).toHaveBeenCalledWith({ tone: "success", message: expect.stringContaining("Importe confirmado: $25.00") });
  });

  it("keeps an in-flight intent isolated when the active user changes", async () => {
    const pending = deferred<InventoryMovement>();
    const onNotice = vi.fn();
    mockedCreate.mockReturnValueOnce(pending.promise);
    const rendered = renderInventory({ onNotice });
    await settleInitial(); await openMovement();
    selectProduct();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "2" } });
    fireEvent.change(screen.getByLabelText(/^Precio de compra/), { target: { value: "12.50" } });
    fireEvent.submit(document.getElementById("inventory-movement-form")!); await act(async () => undefined);
    fireEvent.submit(document.getElementById("inventory-movement-form")!);
    await act(async () => undefined);
    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).not.toBeNull();

    const nextUser = { ...user, id: 8, email: "grace@example.com" };
    rendered.rerender(<InventoryView user={nextUser} onNotice={onNotice} onSessionRequired={vi.fn()} />);
    await act(async () => pending.resolve(movement));

    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).not.toBeNull();
    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${nextUser.id}`)).toBeNull();
    expect(onNotice).not.toHaveBeenCalled();
  });

  it("recovers a legacy v1 intent with the same token", async () => {
    savePendingManualMovement({ version: 1, userId: user.id, operationToken: "legacy-token", productId: 11, reason: "ajuste", type: "salida", quantity: "02" });
    renderInventory(); await settleInitial();
    fireEvent.click(screen.getByRole("button", { name: "Reintentar envio" })); await act(async () => undefined);
    expect(mockedCreate).toHaveBeenCalledWith({ product_id: 11, reason: "ajuste", type: "salida", quantity: "02", operation_token: "legacy-token" });
  });

  it("requotes a legacy v1 entry before opening a v2 draft with the same token", async () => {
    mockedCreate.mockRejectedValueOnce({ category: "conflict", code: "INVENTORY_LEGACY_REQUOTE_REQUIRED", message: "Recotiza" }).mockResolvedValueOnce(movement);
    savePendingManualMovement({ version: 1, userId: user.id, operationToken: "legacy-entry", productId: 11, reason: "compra", type: "entrada", quantity: "2" });
    renderInventory(); await settleInitial();
    fireEvent.click(screen.getByRole("button", { name: "Revisar y completar" }));
    await act(async () => undefined);
    expect(mockedCreate).toHaveBeenCalledWith({ product_id: 11, reason: "compra", type: "entrada", quantity: "2", operation_token: "legacy-entry" });
    fireEvent.change(screen.getByLabelText(/^Precio de compra/), { target: { value: "12.50" } });
    await quoteAndConfirm();
    expect(mockedCreate).toHaveBeenLastCalledWith(expect.objectContaining({ operation_token: "legacy-entry", unit_price: "12.50", quote_fingerprint: "inventory-quote-v1:test" }));
    expect(mockedQuote.mock.invocationCallOrder[0]).toBeLessThan(mockedCreate.mock.invocationCallOrder[1]);
  });

  it("requotes a legacy v1 exit and preserves its token for v2 confirmation", async () => {
    mockedCreate.mockRejectedValueOnce({ category: "conflict", code: "INVENTORY_LEGACY_REQUOTE_REQUIRED", message: "Recotiza" }).mockResolvedValueOnce(movement);
    savePendingManualMovement({ version: 1, userId: user.id, operationToken: "legacy-exit", productId: 11, reason: "ajuste", type: "salida", quantity: "2" });
    renderInventory(); await settleInitial();

    fireEvent.click(screen.getByRole("button", { name: "Reintentar envio" }));
    await act(async () => undefined);

    expect(mockedQuote).toHaveBeenCalledWith({ product_id: 11, presentation_id: null, reason: "ajuste", type: "salida", quantity: "2", unit_price: null });
    expect(mockedCreate).toHaveBeenCalledTimes(1);
    fireEvent.submit(document.getElementById("inventory-movement-form")!);
    await act(async () => undefined);
    expect(mockedCreate).toHaveBeenLastCalledWith(expect.objectContaining({ operation_token: "legacy-exit", quote_fingerprint: "inventory-quote-v1:test" }));
  });

  it.each([
    ["piece", 211, "Pieza", /Piezas individuales/],
    ["gram", 212, "Gramo", /Captura directa en gramos/],
    ["milliliter", 213, "Mililitro", /Captura directa en mililitros/],
  ] as const)("adopts and preserves a fixed factor-one %s presentation when requoting v1", async (baseUnit, presentationId, name, captureName) => {
    const recoveredProduct = {
      ...inventoryRow,
      base_unit: baseUnit,
      presentations: [{ ...presentation, id: presentationId, name, base_quantity: 1 }],
    };
    mockedSearch.mockResolvedValue({ items: [recoveredProduct], page: 1, total: 1, page_size: 20 });
    mockedCreate.mockRejectedValueOnce({ category: "conflict", code: "INVENTORY_LEGACY_REQUOTE_REQUIRED", message: "Recotiza" }).mockResolvedValueOnce(movement);
    savePendingManualMovement({ version: 1, userId: user.id, operationToken: `legacy-${baseUnit}`, productId: 11, reason: "ajuste", type: "salida", quantity: "2" });
    renderInventory(); await settleInitial();

    fireEvent.click(screen.getByRole("button", { name: "Reintentar envio" }));
    await act(async () => undefined);

    expect(screen.getByRole("radio", { name: captureName }).getAttribute("aria-checked")).toBe("true");
    expect(mockedQuote).toHaveBeenCalledWith(expect.objectContaining({ product_id: 11, presentation_id: presentationId }));
    fireEvent.submit(document.getElementById("inventory-movement-form")!);
    await act(async () => undefined);
    expect(mockedCreate).toHaveBeenLastCalledWith(expect.objectContaining({ operation_token: `legacy-${baseUnit}`, presentation_id: presentationId }));
  });

  it("clears a rejected validation safely, preserves the draft and uses a new token", async () => {
    mockedCreate.mockRejectedValue({ category: "validation", code: "INVALID_INVENTORY_COST", message: "Costo invalido", field: "unit_price" });
    renderInventory(); await settleInitial(); await openMovement();
    selectProduct();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "2" } });
    const cost = screen.getByLabelText(/^Precio de compra/);
    fireEvent.change(cost, { target: { value: "12.50" } }); await quoteAndConfirm();
    await act(async () => undefined); await act(() => vi.runAllTimersAsync());
    expect(cost.getAttribute("aria-invalid")).toBe("true"); expect(document.activeElement).toBe(cost);
    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).toBeNull();
    const rejectedToken = vi.mocked(mockedCreate).mock.calls[0][0].operation_token;
    mockedCreate.mockResolvedValue(movement);
    fireEvent.change(cost, { target: { value: "13.00" } }); await quoteAndConfirm();
    expect(vi.mocked(mockedCreate).mock.calls[1][0].operation_token).not.toBe(rejectedToken);
  });

  it("keeps an uncertain failure available for an idempotent retry", async () => {
    mockedCreate.mockRejectedValue(new Error("connection closed"));
    renderInventory(); await settleInitial(); await openMovement();
    selectProduct();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "2" } });
    fireEvent.change(screen.getByLabelText(/^Precio de compra/), { target: { value: "12.50" } });
    await quoteAndConfirm();
    const stored = JSON.parse(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)!);
    expect(stored.operationToken).toBe(vi.mocked(mockedCreate).mock.calls[0][0].operation_token);
    fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(screen.getByRole("button", { name: "Reintentar envio" })).toBeTruthy();
  });

  it("reopens v2 metadata and requires a new confirmation when the quote changed", async () => {
    savePendingManualMovement({
      version: 2, userId: user.id, operationToken: "metadata-token", productId: 11,
      presentationId: 101, reason: "compra", type: "entrada", quantity: "2", unitPrice: "12.50",
      supplier: "Proveedor reabierto", documentReference: "FAC-REOPEN", note: "Nota reabierta",
      quoteFingerprint: "inventory-quote-v1:old",
    });
    mockedCreate
      .mockRejectedValueOnce({ category: "conflict", code: "QUOTE_CHANGED", message: "Cambio" })
      .mockResolvedValueOnce(movement);
    mockedQuote.mockResolvedValueOnce(quote({ total_cost_cents: 2600, quote_fingerprint: "inventory-quote-v1:new" }));

    renderInventory(); await settleInitial();
    fireEvent.click(screen.getByRole("button", { name: "Reintentar envio" }));
    await act(async () => undefined);

    expect((screen.getByLabelText("Proveedor") as HTMLInputElement).value).toBe("Proveedor reabierto");
    expect((screen.getByLabelText("Factura o referencia") as HTMLInputElement).value).toBe("FAC-REOPEN");
    expect((screen.getByLabelText("Nota") as HTMLTextAreaElement).value).toBe("Nota reabierta");
    expect(screen.getByText((_, element) => element?.textContent === "Importe total: $26.00")).toBeTruthy();
    expect(mockedCreate).toHaveBeenCalledTimes(1);

    fireEvent.submit(document.getElementById("inventory-movement-form")!);
    await act(async () => undefined);
    expect(mockedCreate).toHaveBeenCalledTimes(2);
    expect(mockedCreate).toHaveBeenLastCalledWith(expect.objectContaining({
      operation_token: "metadata-token",
      quote_fingerprint: "inventory-quote-v1:new",
      supplier: "Proveedor reabierto",
      document_reference: "FAC-REOPEN",
      note: "Nota reabierta",
    }));
  });

  it("rehydrates an intent product and its capture forms across catalog pages before requoting", async () => {
    savePendingManualMovement({
      version: 2, userId: user.id, operationToken: "paged-recovery", productId: 11,
      presentationId: 101, reason: "compra", type: "entrada", quantity: "2", unitPrice: "12.50",
      supplier: null, documentReference: null, note: null, quoteFingerprint: "inventory-quote-v1:old",
    });
    mockedSearch.mockImplementation(async (_search, page) => page === 1
      ? { items: [emptyRow], page: 1, total: 21, page_size: 20 }
      : { items: [inventoryRow], page: 2, total: 21, page_size: 20 });
    mockedCreate.mockRejectedValueOnce({ category: "conflict", code: "QUOTE_CHANGED", message: "Cambio" });
    mockedQuote.mockImplementationOnce(async () => {
      expect(screen.getByLabelText("Producto seleccionado").textContent).toContain("Cafe molido");
      expect(screen.getByRole("radio", { name: /Bolsa 250 g/ }).getAttribute("aria-checked")).toBe("true");
      return quote({ quote_fingerprint: "inventory-quote-v1:new" });
    });

    renderInventory(); await settleInitial();
    fireEvent.click(screen.getByRole("button", { name: "Reintentar envio" }));
    await act(async () => undefined); await act(async () => undefined);

    expect(mockedSearch).toHaveBeenNthCalledWith(1, undefined, 1);
    expect(mockedSearch).toHaveBeenNthCalledWith(2, undefined, 2);
    expect(mockedSearch.mock.invocationCallOrder[1]).toBeLessThan(mockedQuote.mock.invocationCallOrder[0]);
    expect(screen.getByText((_, element) => element?.textContent === "Forma de captura: Bolsa 250 g")).toBeTruthy();
  });

  it("labels measured capture explicitly and searches the selector remotely", async () => {
    const measured = { ...presentation, id: 102, name: "Kilogramo", mode: "measured" as const, base_quantity: 1000 };
    mockedSearch.mockResolvedValue({ items: [{ ...inventoryRow, presentations: [measured] }], page: 1, total: 1, page_size: 20 });
    renderInventory(); await settleInitial(); await openMovement();
    fireEvent.change(screen.getAllByLabelText("Buscar producto")[1], { target: { value: "cafe" } });
    await settleInitial();
    expect(mockedSearch).toHaveBeenCalledWith("cafe", 1);
    selectProduct();
    fireEvent.click(screen.getByRole("radio", { name: /Kilogramo/ }));
    expect(screen.getByLabelText(/^Cantidad en gramos/)).toBeTruthy();
  });

  it("supports keyboard navigation, selection, escape and changing the selected product", async () => {
    renderInventory(); await settleInitial(); await openMovement();
    const dialog = screen.getByRole("dialog");
    const combobox = within(dialog).getByRole("combobox", { name: "Buscar producto" });
    fireEvent.keyDown(combobox, { key: "ArrowDown" });
    fireEvent.keyDown(combobox, { key: "Enter" });
    expect(within(dialog).getByLabelText("Producto seleccionado").textContent).toContain("Producto sin movimientos");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cambiar producto" }));
    const reopenedCombobox = within(dialog).getByLabelText("Buscar producto");
    fireEvent.keyDown(reopenedCombobox, { key: "Escape" });
    expect(reopenedCombobox.getAttribute("aria-expanded")).toBe("false");
    await act(() => vi.advanceTimersByTimeAsync(250));
    expect(reopenedCombobox.getAttribute("aria-expanded")).toBe("false");
  });

  it("uses roving tabindex and selection-following-focus for capture forms", async () => {
    renderInventory(); await settleInitial(); await openMovement(); selectProduct();
    const radios = screen.getAllByRole("radio");
    expect(radios).toHaveLength(2);
    expect(radios[0].getAttribute("tabindex")).toBe("0");
    expect(radios[1].getAttribute("tabindex")).toBe("-1");

    radios[0].focus();
    fireEvent.keyDown(radios[0], { key: "ArrowRight" });
    await act(() => vi.runAllTimersAsync());
    expect(radios[1].getAttribute("aria-checked")).toBe("true");
    expect(radios[1].getAttribute("tabindex")).toBe("0");
    expect(document.activeElement).toBe(radios[1]);

    fireEvent.keyDown(radios[1], { key: "Home" });
    await act(() => vi.runAllTimersAsync());
    expect(radios[0].getAttribute("aria-checked")).toBe("true");
    expect(document.activeElement).toBe(radios[0]);
    fireEvent.keyDown(radios[0], { key: "End" });
    await act(() => vi.runAllTimersAsync());
    expect(document.activeElement).toBe(radios[1]);
  });

  it("discards stale searches and keeps previous results visible while reloading", async () => {
    const oldRequest = deferred<Awaited<ReturnType<typeof searchInventoryProducts>>>();
    const newRequest = deferred<Awaited<ReturnType<typeof searchInventoryProducts>>>();
    renderInventory(); await settleInitial(); await openMovement();
    const combobox = within(screen.getByRole("dialog")).getByRole("combobox", { name: "Buscar producto" });
    mockedSearch.mockReset().mockReturnValueOnce(oldRequest.promise).mockReturnValueOnce(newRequest.promise);
    fireEvent.change(combobox, { target: { value: "anterior" } });
    expect(screen.queryByRole("button", { name: "Cargar mas" })).toBeNull();
    expect(screen.getByRole("option", { name: /Cafe molido/ }).hasAttribute("disabled")).toBe(true);
    await act(() => vi.advanceTimersByTimeAsync(250));
    expect(screen.getByRole("option", { name: /Cafe molido/ })).toBeTruthy();
    expect(screen.getByText("Actualizando resultados anteriores...")).toBeTruthy();
    fireEvent.change(combobox, { target: { value: "nuevo" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => newRequest.resolve({ items: [emptyRow], page: 1, total: 1, page_size: 20 }));
    expect(screen.getByRole("option", { name: /Producto sin movimientos/ })).toBeTruthy();
    await act(async () => oldRequest.resolve({ items: [inventoryRow], page: 1, total: 1, page_size: 20 }));
    expect(screen.queryByRole("option", { name: /Cafe molido/ })).toBeNull();
  });

  it("shows empty and retry states and appends the next page without duplicates", async () => {
    mockedSearch.mockRejectedValueOnce(new Error("private SQL")).mockResolvedValueOnce({ items: [], page: 1, total: 0, page_size: 20 });
    renderInventory(); await settleInitial(); await openMovement();
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByRole("alert").textContent).toContain("No se pudieron cargar");
    fireEvent.click(within(dialog).getByRole("button", { name: "Reintentar" }));
    await act(async () => undefined);
    expect(within(dialog).getByText("No se encontraron productos.")).toBeTruthy();

    mockedSearch.mockResolvedValueOnce({ items: [inventoryRow], page: 1, total: 21, page_size: 20 });
    fireEvent.change(within(dialog).getByRole("combobox", { name: "Buscar producto" }), { target: { value: "cafe" } });
    await settleInitial();
    mockedSearch.mockResolvedValueOnce({ items: [inventoryRow, emptyRow], page: 2, total: 21, page_size: 20 });
    const combobox = within(dialog).getByRole("combobox", { name: "Buscar producto" });
    expect(combobox.getAttribute("aria-activedescendant")).toBe("inventory-product-option-11");
    expect(within(dialog).getByRole("option", { name: /Cafe molido/ }).getAttribute("tabindex")).toBe("-1");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cargar mas" }));
    await act(async () => undefined);
    await act(() => vi.runAllTimersAsync());
    expect(within(within(dialog).getByRole("listbox")).getAllByRole("option")).toHaveLength(2);
    expect(combobox.getAttribute("aria-activedescendant")).toBe("inventory-product-option-11");
    expect(within(dialog).queryByRole("button", { name: "Cargar mas" })).toBeNull();
    expect(document.activeElement).toBe(combobox);

    const lastOption = within(dialog).getByRole("option", { name: /Producto sin movimientos/ });
    const scrollIntoView = vi.fn();
    lastOption.scrollIntoView = scrollIntoView;
    fireEvent.keyDown(combobox, { key: "ArrowDown" });
    expect(combobox.getAttribute("aria-activedescendant")).toBe("inventory-product-option-22");
    expect(scrollIntoView).toHaveBeenCalledWith({ block: "nearest" });
    expect(lastOption.getAttribute("tabindex")).toBe("-1");
  });

  it("retries a failed next page without replacing previous results", async () => {
    mockedSearch.mockResolvedValueOnce({ items: [inventoryRow], page: 1, total: 21, page_size: 20 });
    renderInventory(); await settleInitial(); await openMovement();
    const dialog = screen.getByRole("dialog");
    mockedSearch.mockRejectedValueOnce(new Error("page failed"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Cargar mas" }));
    await act(async () => undefined);
    await act(() => vi.runAllTimersAsync());
    expect(within(dialog).getByRole("option", { name: /Cafe molido/ })).toBeTruthy();
    expect(document.activeElement).toBe(within(dialog).getByRole("combobox", { name: "Buscar producto" }));

    mockedSearch.mockResolvedValueOnce({ items: [emptyRow], page: 2, total: 21, page_size: 20 });
    fireEvent.click(within(dialog).getByRole("button", { name: "Reintentar" }));
    await act(async () => undefined);

    expect(mockedSearch).toHaveBeenLastCalledWith(undefined, 2);
    expect(within(within(dialog).getByRole("listbox")).getAllByRole("option")).toHaveLength(2);
  });

  it("deduplicates an equivalent base presentation and keeps its presentation id", async () => {
    const piece = { ...emptyRow, presentations: [
      { ...presentation, id: 201, product_id: 22, name: "Pieza", base_quantity: 1 },
      { ...presentation, id: 202, product_id: 22, name: "Pieza duplicada", base_quantity: 1 },
    ] };
    mockedSearch.mockResolvedValue({ items: [piece], page: 1, total: 1, page_size: 20 });
    mockedQuote.mockResolvedValue(quote({ product_id: 22, product_name: piece.name, product_sku: piece.sku, presentation_id: 201, presentation_name: "Pieza", capture_name: "Piezas individuales", presentation_base_quantity: 1, entered_quantity: 3, entered_quantity_description: "3 x Pieza", base_quantity: 3, base_quantity_description: "3 piezas", base_unit: "piece" }));
    renderInventory(); await settleInitial(); await openMovement(); selectProduct("Producto sin movimientos");
    expect(screen.getAllByRole("radio", { name: /Piezas individuales/ })).toHaveLength(1);
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "3" } });
    fireEvent.change(screen.getByLabelText(/^Precio de compra/), { target: { value: "1.00" } });
    fireEvent.submit(document.getElementById("inventory-movement-form")!); await act(async () => undefined);
    expect(mockedQuote).toHaveBeenCalledWith(expect.objectContaining({ product_id: 22, presentation_id: 201 }));
  });

  it("labels a fixed factor-one weight presentation as direct capture", async () => {
    const directWeight = { ...presentation, id: 203, name: "Gramo", base_quantity: 1 };
    mockedSearch.mockResolvedValue({ items: [{ ...inventoryRow, presentations: [directWeight] }], page: 1, total: 1, page_size: 20 });
    renderInventory(); await settleInitial(); await openMovement(); selectProduct();

    expect(screen.getByRole("radio", { name: /Captura directa en gramos/ }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByLabelText(/^Cantidad en gramos/)).toBeTruthy();
    expect(screen.getByLabelText(/^Precio de compra por gramo/)).toBeTruthy();
    expect(screen.getByText("Captura directa en la unidad indicada.")).toBeTruthy();
  });

  it("labels direct piece capture without a fixed factor-one presentation", async () => {
    const piece = { ...emptyRow, presentations: [{ ...presentation, id: 204, product_id: 22, name: "Caja", base_quantity: 6 }] };
    mockedSearch.mockResolvedValue({ items: [piece], page: 1, total: 1, page_size: 20 });
    renderInventory(); await settleInitial(); await openMovement(); selectProduct("Producto sin movimientos");

    expect(screen.getByRole("radio", { name: /Piezas individuales/ }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByLabelText(/^Cantidad en piezas/)).toBeTruthy();
  });

  it("renders the confirmation identity and amounts exclusively from the Rust quote", async () => {
    mockedQuote.mockResolvedValue(quote({ product_name: "Nombre resuelto por Rust", product_sku: "RUST-SKU", capture_name: "Captura resuelta", entered_quantity_description: "7 paquetes resueltos", base_quantity_description: "700 piezas", unit_price_cents: 5000, unit_price_description: "por paquete resuelto", total_cost_cents: 35000 }));
    renderInventory(); await settleInitial(); await openMovement(); selectProduct();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "7" } });
    fireEvent.change(screen.getByLabelText(/^Precio de compra/), { target: { value: "50.00" } });
    fireEvent.submit(document.getElementById("inventory-movement-form")!); await act(async () => undefined);
    const summary = screen.getByText("Resumen por confirmar").parentElement!;
    expect(summary.textContent).toContain("Nombre resuelto por Rust");
    expect(summary.textContent).toContain("Captura resuelta");
    expect(summary.textContent).toContain("7 paquetes resueltos");
    expect(summary.textContent).toContain("700 piezas");
    expect(summary.textContent).toContain("$350.00");
  });

  it("keeps stale table data on refresh errors and supports retry", async () => {
    renderInventory(); await settleInitial();
    mockedView.mockRejectedValueOnce(new Error("SQLITE private")).mockResolvedValueOnce(inventory);
    fireEvent.change(screen.getByLabelText("Buscar producto"), { target: { value: "nuevo" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    const alert = screen.getByRole("alert"); expect(alert.textContent).toContain("No se pudo consultar"); expect(alert.textContent).not.toContain("SQLITE");
    expect(screen.getAllByText("Cafe molido").length).toBeGreaterThan(0);
    fireEvent.click(within(alert).getByRole("button", { name: "Reintentar" })); await act(async () => undefined);
    expect(mockedView).toHaveBeenCalled();
  });
});
