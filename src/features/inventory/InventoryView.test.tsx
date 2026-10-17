import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createInventoryMovement, getInventoryView, getProductInventoryDetail, quoteInventoryMovement, searchInventoryProducts } from "../../api/inventory";
import { deferred, user } from "../../test/fixtures";
import type { InventoryMovement, InventoryMovementQuote, InventoryView as InventoryViewData, ProductInventoryDetail } from "../../types/inventory_movement";
import { InventoryView } from "./InventoryView";
import { savePendingManualMovement } from "./manual-movement-intent";

vi.mock("../../api/inventory", () => ({ createInventoryMovement: vi.fn(), getInventoryView: vi.fn(), getProductInventoryDetail: vi.fn(), quoteInventoryMovement: vi.fn(), searchInventoryProducts: vi.fn() }));

const movement: InventoryMovement = {
  id: 1, product_id: 11, product_name: "Cable actual", sku: "CABLE-NUEVO", product_name_snapshot: "Cable historico", sku_snapshot: "CABLE-ANTERIOR",
  sale_id: null, ticket_reference: null, record_version: 2, origin: "manual", presentation_id: 101, presentation_name: "Metro", presentation_barcode: null,
  base_unit: "centimeter", presentation_mode: "measured", presentation_base_quantity: 100, unit_price_cents: 2500,
  reason: "compra", type: "entrada", quantity: 75, entered_quantity: 75, base_quantity: 75, stock_before: 100, stock_after: 175,
  movement_cost_cents: 1875, inventory_cost_before_cents: 2500, inventory_cost_after_cents: 4375,
  supplier: "Proveedor", document_reference: "FAC-1", note: "Lote", operation_token: "token",
  created_at: "2026-10-01T10:00:00Z", updated_at: "2026-10-01T10:00:00Z",
};
const presentation = { id: 101, product_id: 11, name: "Metro", mode: "measured" as const, base_quantity: 100, is_active: true };
const inventoryRow = { id: 11, name: "Cable", sku: "CABLE-1", stock: 175, base_unit: "centimeter" as const, inventory_cost_cents: 4375, inventory_cost_status: "known" as const, average_cost_cents: 25, presentations: [presentation] };
const pieceRow = { id: 22, name: "Tornillo", sku: "TOR-1", stock: 7, base_unit: "piece" as const, inventory_cost_cents: 700, inventory_cost_status: "known" as const, average_cost_cents: 100, presentations: [] };
const inventory: InventoryViewData = {
  stats: { product_count: 2, piece_stock: 7, gram_stock: 1250, milliliter_stock: 2000, centimeter_stock: 175, inventory_value_cents: 5075, products_without_cost: 0 },
  products: [inventoryRow, pieceRow], movements: [movement], items: [movement], product_page: 1, product_total: 21, movement_page: 1, movement_total: 1, page_size: 20,
};
const detail: ProductInventoryDetail = { product: inventoryRow, last_movement_at: movement.created_at, movements: [movement], page: 1, total: 1, page_size: 20 };

const mockedView = vi.mocked(getInventoryView);
const mockedCreate = vi.mocked(createInventoryMovement);
const mockedQuote = vi.mocked(quoteInventoryMovement);
const mockedDetail = vi.mocked(getProductInventoryDetail);
const mockedSearch = vi.mocked(searchInventoryProducts);

function quote(overrides: Partial<InventoryMovementQuote> = {}): InventoryMovementQuote {
  return {
    product_id: 11, product_name: "Cable", product_sku: "CABLE-1", reason: "compra", type: "entrada",
    presentation_id: 101, presentation_name: "Metro", capture_name: "Metro", presentation_mode: "measured", presentation_base_quantity: 100,
    entered_quantity: 75, entered_quantity_description: "75 cm", base_quantity: 75, base_quantity_description: "75 cm",
    unit_price_cents: 2500, unit_price_description: "por 1 m", total_cost_cents: 1875, base_unit: "centimeter",
    quote_fingerprint: "inventory-quote-v4:test", ...overrides,
  };
}

function renderInventory(props: Partial<React.ComponentProps<typeof InventoryView>> = {}) {
  return render(<InventoryView user={user} onNotice={vi.fn()} onSessionRequired={vi.fn()} {...props} />);
}
async function settleInitial() { await act(() => vi.advanceTimersByTimeAsync(250)); await act(async () => undefined); }
async function openMovement() { fireEvent.click(screen.getByRole("button", { name: "Registrar movimiento" })); await settleInitial(); }
function selectProduct(name = "Cable") { fireEvent.click(screen.getByRole("option", { name: new RegExp(name) })); }
async function submitForm() { fireEvent.submit(document.getElementById("inventory-movement-form")!); await act(async () => undefined); }

beforeEach(() => {
  vi.useFakeTimers();
  localStorage.clear();
  mockedView.mockReset().mockResolvedValue(inventory);
  mockedSearch.mockReset().mockResolvedValue({ items: inventory.products, page: 1, total: inventory.product_total, page_size: 20 });
  mockedQuote.mockReset().mockResolvedValue(quote());
  mockedCreate.mockReset().mockResolvedValue(movement);
  mockedDetail.mockReset().mockResolvedValue(detail);
});

describe("InventoryView", () => {
  it("shows centimeter stock and accumulated inventory cost", async () => {
    renderInventory(); await settleInitial();
    expect(screen.getByText("Existencia por longitud")).toBeTruthy();
    expect(screen.getAllByText("1.75 m").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Valor a costo").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("$50.75")).toBeTruthy();
  });

  it("offers centimeters as a filter and keeps independent pages and filters", async () => {
    renderInventory(); await settleInitial();
    fireEvent.change(screen.getByLabelText("Unidad base"), { target: { value: "centimeter" } });
    fireEvent.change(screen.getByLabelText("Buscar producto"), { target: { value: "cable" } });
    fireEvent.click(screen.getByRole("button", { name: "Siguiente" }));
    fireEvent.click(screen.getByRole("tab", { name: "Movimientos" }));
    fireEvent.change(screen.getByLabelText("Buscar movimiento"), { target: { value: "FAC" } });
    await settleInitial();
    expect(mockedView).toHaveBeenLastCalledWith(expect.objectContaining({ base_unit: "centimeter", product_search: "cable", product_page: 2, movement_search: "FAC", movement_page: 1 }));
  });

  it("shows movement snapshot identity instead of the current product identity", async () => {
    renderInventory(); await settleInitial();
    fireEvent.click(screen.getByRole("tab", { name: "Movimientos" }));
    const panel = screen.getByRole("tabpanel", { name: "Movimientos" });
    expect(within(panel).getByText("Cable historico")).toBeTruthy();
    expect(within(panel).getByText("CABLE-ANTERIOR")).toBeTruthy();
    expect(within(panel).queryByText("Cable actual")).toBeNull();
    expect(within(panel).queryByText("CABLE-NUEVO")).toBeNull();
  });

  it("quotes purchase price and persists v4 metadata", async () => {
    mockedCreate.mockImplementation(async (input) => {
      const stored = JSON.parse(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)!);
      expect(stored).toEqual(expect.objectContaining({ version: 4, unitPrice: "25.00", supplier: "Proveedor", quoteFingerprint: "inventory-quote-v4:test" }));
      expect(input.operation_token).toBe(stored.operationToken);
      return movement;
    });
    renderInventory(); await settleInitial(); await openMovement(); selectProduct();
    fireEvent.click(screen.getByRole("radio", { name: /Metro/ }));
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "75" } });
    fireEvent.change(screen.getByLabelText(/Precio de compra/), { target: { value: "25.00" } });
    fireEvent.change(screen.getByLabelText("Proveedor"), { target: { value: "Proveedor" } });
    fireEvent.change(screen.getByLabelText("Factura o referencia"), { target: { value: "FAC-1" } });
    fireEvent.change(screen.getByLabelText("Nota"), { target: { value: "Lote" } });
    expect(screen.getByLabelText(/Precio de compra/)).toBeTruthy();
    expect(within(screen.getByRole("dialog")).queryByRole("option", { name: /costo actual/i })).toBeNull();
    await submitForm();
    expect(mockedQuote).toHaveBeenCalledWith({ product_id: 11, presentation_id: 101, reason: "compra", type: "entrada", quantity: "75", unit_price: "25.00", supplier: "Proveedor", document_reference: "FAC-1", note: "Lote" });
    expect(screen.getByText((_, node) => node?.textContent === "Precio de compra: $25.00 por 1 m")).toBeTruthy();
    expect(screen.getByText((_, node) => node?.textContent === "Costo del movimiento: $18.75")).toBeTruthy();
    await submitForm();
    expect(mockedCreate).toHaveBeenCalledWith(expect.objectContaining({ unit_price: "25.00", supplier: "Proveedor", document_reference: "FAC-1", note: "Lote", quote_fingerprint: "inventory-quote-v4:test" }));
  });

  it("selects a product from the combobox with arrow keys and Enter", async () => {
    renderInventory(); await settleInitial(); await openMovement();
    const combobox = screen.getByRole("combobox", { name: "Buscar producto" });
    combobox.focus();
    fireEvent.keyDown(combobox, { key: "ArrowDown" });
    expect(combobox.getAttribute("aria-activedescendant")).toBe("inventory-product-option-22");
    fireEvent.keyDown(combobox, { key: "Enter" });
    expect(screen.getByRole("region", { name: "Producto seleccionado" }).textContent).toContain("Tornillo");
  });

  it("moves and selects capture radios with arrow keys", async () => {
    renderInventory(); await settleInitial(); await openMovement(); selectProduct();
    const direct = screen.getByRole("radio", { name: /Captura directa en centimetros/ });
    const meter = screen.getByRole("radio", { name: /Metro/ });
    direct.focus();
    fireEvent.keyDown(direct, { key: "ArrowRight" });
    await act(() => vi.advanceTimersByTimeAsync(16));
    expect(meter.getAttribute("aria-checked")).toBe("true");
    expect(document.activeElement).toBe(meter);
    fireEvent.keyDown(meter, { key: "ArrowLeft" });
    await act(() => vi.advanceTimersByTimeAsync(16));
    expect(direct.getAttribute("aria-checked")).toBe("true");
    expect(document.activeElement).toBe(direct);
  });

  it("dismisses the combobox first, then closes the dialog and restores trigger focus", async () => {
    const visibleRects = vi.spyOn(HTMLElement.prototype, "getClientRects").mockReturnValue({ length: 1 } as DOMRectList);
    renderInventory(); await settleInitial();
    const trigger = screen.getByRole("button", { name: "Registrar movimiento" });
    trigger.focus();
    await openMovement();
    const combobox = screen.getByRole("combobox", { name: "Buscar producto" });
    combobox.focus();
    fireEvent.keyDown(combobox, { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "Registrar movimiento" })).toBeTruthy();
    expect(combobox.getAttribute("aria-expanded")).toBe("false");
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Registrar movimiento" })).toBeNull();
    expect(document.activeElement).toBe(trigger);
    visibleRects.mockRestore();
  });

  it("invalidates a quote when operational metadata changes", async () => {
    renderInventory(); await settleInitial(); await openMovement(); selectProduct();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "75" } });
    await submitForm();
    expect(screen.getByText("Resumen por confirmar")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Proveedor"), { target: { value: "Nuevo proveedor" } });
    expect(screen.queryByText("Resumen por confirmar")).toBeNull();
    await submitForm();
    expect(mockedQuote).toHaveBeenLastCalledWith(expect.objectContaining({ supplier: "Nuevo proveedor" }));
    expect(mockedCreate).not.toHaveBeenCalled();
  });

  it("does not capture purchase price for outgoing adjustments", async () => {
    mockedQuote.mockResolvedValueOnce(quote({ reason: "ajuste", type: "salida", unit_price_cents: 25, unit_price_description: "por centimetro" }));
    renderInventory(); await settleInitial(); await openMovement(); selectProduct();
    fireEvent.change(screen.getByLabelText(/^Operacion/), { target: { value: "adjustment-out" } });
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "25" } });
    expect(screen.queryByLabelText(/Precio de compra/)).toBeNull();
    await submitForm();
    expect(mockedQuote).toHaveBeenCalledWith(expect.objectContaining({ reason: "ajuste", type: "salida", quantity: "25", unit_price: null }));
  });

  it("quotes and confirms valuation without changing quantity or presentation", async () => {
    mockedQuote.mockResolvedValueOnce(quote({ reason: "valoracion", type: "valoracion", presentation_id: null, presentation_name: null, presentation_mode: null, presentation_base_quantity: 1, entered_quantity: 0, entered_quantity_description: "0 centimetros", base_quantity: 0, base_quantity_description: "0 centimetros", unit_price_cents: 5000, unit_price_description: "valor total del inventario", total_cost_cents: 5000 }));
    renderInventory(); await settleInitial(); await openMovement(); selectProduct();
    fireEvent.change(screen.getByLabelText(/^Operacion/), { target: { value: "valuation" } });
    expect(screen.queryByLabelText(/^Cantidad/)).toBeNull();
    fireEvent.change(screen.getByLabelText(/^Nuevo costo acumulado del inventario/), { target: { value: "50.00" } });
    await submitForm();
    expect(mockedQuote).toHaveBeenCalledWith(expect.objectContaining({ presentation_id: null, reason: "valoracion", type: "valoracion", quantity: "0", unit_price: "50.00" }));
    await submitForm();
    expect(mockedCreate).toHaveBeenCalledWith(expect.objectContaining({ presentation_id: null, reason: "valoracion", type: "valoracion", quantity: "0", unit_price: "50.00" }));
  });

  it("loads v2 context, requotes and replaces it with v4 before create", async () => {
    savePendingManualMovement({ version: 2, userId: user.id, operationToken: "legacy-v2", productId: 11, presentationId: 101, reason: "compra", type: "entrada", quantity: "75", unitPrice: "999.99", supplier: "Proveedor viejo", documentReference: "FAC-V2", note: "Nota", quoteFingerprint: "inventory-quote-v2:old" });
    mockedCreate.mockImplementation(async () => {
      const stored = JSON.parse(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)!);
      expect(stored.version).toBe(4);
      expect(stored.operationToken).toBe("legacy-v2");
      expect(stored.unitPrice).toBe("999.99");
      return movement;
    });
    renderInventory(); await settleInitial();
    fireEvent.click(screen.getByRole("button", { name: "Revisar y completar" })); await act(async () => undefined);
    expect(mockedCreate).not.toHaveBeenCalled();
    expect(mockedQuote).toHaveBeenCalledWith(expect.objectContaining({ unit_price: "999.99", supplier: "Proveedor viejo", document_reference: "FAC-V2", note: "Nota" }));
    expect((screen.getByLabelText("Proveedor") as HTMLInputElement).value).toBe("Proveedor viejo");
    await submitForm();
    expect(mockedCreate).toHaveBeenCalledOnce();
  });

  it("loads no-cost v3 only as context and requires price re-entry and requote", async () => {
    savePendingManualMovement({ version: 3, userId: user.id, operationToken: "discarded-v3", productId: 11, presentationId: 101, reason: "compra", type: "entrada", quantity: "75", supplier: "Proveedor", documentReference: "FAC-V3", note: "Nota", quoteFingerprint: "inventory-quote-v3:discarded" });
    renderInventory(); await settleInitial();
    fireEvent.click(screen.getByRole("button", { name: "Revisar y completar" })); await act(async () => undefined);
    expect(mockedCreate).not.toHaveBeenCalled();
    expect(mockedQuote).not.toHaveBeenCalled();
    const price = screen.getByLabelText(/Precio de compra/) as HTMLInputElement;
    expect(price.value).toBe("");
    await act(() => vi.advanceTimersByTimeAsync(16));
    expect(document.activeElement).toBe(price);
    fireEvent.change(price, { target: { value: "25.00" } });
    await submitForm();
    expect(mockedQuote).toHaveBeenCalledWith(expect.objectContaining({ unit_price: "25.00" }));
    expect(mockedCreate).not.toHaveBeenCalled();
    await submitForm();
    expect(mockedCreate).toHaveBeenCalledWith(expect.objectContaining({ operation_token: "discarded-v3", unit_price: "25.00", quote_fingerprint: "inventory-quote-v4:test" }));
  });

  it("can preserve and safely retry recovered v3 context after entering the discard transition", async () => {
    savePendingManualMovement({ version: 3, userId: user.id, operationToken: "discard-v3", productId: 11, presentationId: 101, reason: "compra", type: "entrada", quantity: "75", supplier: "Proveedor", documentReference: "FAC-V3", note: "Nota", quoteFingerprint: "inventory-quote-v3:discarded" });
    mockedCreate.mockRejectedValueOnce({ category: "conflict", code: "STOCK_CHANGED", message: "El stock cambio" });
    renderInventory(); await settleInitial();
    fireEvent.click(screen.getByRole("button", { name: "Revisar y completar" })); await act(async () => undefined);
    fireEvent.change(screen.getByLabelText(/Precio de compra/), { target: { value: "25.00" } });
    await submitForm();
    await submitForm();
    fireEvent.click(within(screen.getByRole("dialog", { name: "Registrar movimiento" })).getByRole("button", { name: "Descartar pendiente" }));
    expect(screen.getByRole("dialog", { name: "Descartar movimiento pendiente" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Conservar" }));
    expect(JSON.parse(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)!).operationToken).toBe("discard-v3");
    expect(screen.getByRole("button", { name: "Reintentar envio" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Reintentar envio" }));
    await act(async () => undefined);
    expect(mockedCreate).toHaveBeenCalledTimes(2);
    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).toBeNull();
  });

  it("requotes v1 non-piece context without adopting an old fixed equivalent", async () => {
    const legacyFixed = { id: 102, product_id: 11, name: "Centimetro antiguo", mode: "fixed" as const, base_quantity: 1, is_active: true };
    mockedSearch.mockResolvedValue({ items: [{ ...inventoryRow, presentations: [legacyFixed, presentation] }], page: 1, total: 1, page_size: 20 });
    savePendingManualMovement({ version: 1, userId: user.id, operationToken: "legacy-v1", productId: 11, reason: "ajuste", type: "salida", quantity: "2" });
    renderInventory(); await settleInitial();
    fireEvent.click(screen.getByRole("button", { name: "Revisar y completar" })); await act(async () => undefined);
    expect(mockedQuote).toHaveBeenCalledWith(expect.objectContaining({ presentation_id: null, supplier: null, document_reference: null, note: null }));
    expect(screen.queryByRole("radio", { name: /Centimetro antiguo/ })).toBeNull();
    expect(screen.getByRole("radio", { name: /Captura directa en centimetros/ })).toBeTruthy();
  });

  it("drops an invalid fixed non-piece presentation from a v2 recovery", async () => {
    const legacyFixed = { id: 102, product_id: 11, name: "Centimetro antiguo", mode: "fixed" as const, base_quantity: 1, is_active: true };
    mockedSearch.mockResolvedValue({ items: [{ ...inventoryRow, presentations: [legacyFixed, presentation] }], page: 1, total: 1, page_size: 20 });
    savePendingManualMovement({ version: 2, userId: user.id, operationToken: "legacy-fixed-v2", productId: 11, presentationId: 102, reason: "compra", type: "entrada", quantity: "2", unitPrice: "1.00", supplier: null, documentReference: null, note: null, quoteFingerprint: "old" });
    renderInventory(); await settleInitial();
    fireEvent.click(screen.getByRole("button", { name: "Revisar y completar" })); await act(async () => undefined);
    expect(mockedQuote).toHaveBeenCalledWith(expect.objectContaining({ presentation_id: null }));
    expect(screen.queryByRole("radio", { name: /Centimetro antiguo/ })).toBeNull();
  });

  it("does not offer fixed capture for non-piece products but lists historical snapshots safely", async () => {
    const legacyFixed = { id: 102, product_id: 11, name: "Rollo antiguo", mode: "fixed" as const, base_quantity: 250, is_active: true };
    mockedSearch.mockResolvedValue({ items: [{ ...inventoryRow, presentations: [legacyFixed, presentation] }], page: 1, total: 1, page_size: 20 });
    mockedView.mockResolvedValue({ ...inventory, movements: [{ ...movement, record_version: 2, presentation_name: "Rollo antiguo", presentation_mode: "fixed", unit_price_cents: null, movement_cost_cents: null }] });
    renderInventory(); await settleInitial();
    fireEvent.click(screen.getByRole("tab", { name: "Movimientos" }));
    expect(screen.getByText("Desconocido")).toBeTruthy();
    await openMovement(); selectProduct();
    expect(screen.queryByRole("radio", { name: /Rollo antiguo/ })).toBeNull();
  });

  it("keeps an in-flight v3 intent isolated when the active user changes", async () => {
    const pending = deferred<InventoryMovement>();
    const onNotice = vi.fn();
    mockedCreate.mockReturnValueOnce(pending.promise);
    const rendered = renderInventory({ onNotice });
    await settleInitial(); await openMovement(); selectProduct();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "75" } });
    fireEvent.change(screen.getByLabelText(/Precio de compra/), { target: { value: "25.00" } });
    await submitForm(); await submitForm();
    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).not.toBeNull();
    const nextUser = { ...user, id: 8, email: "grace@example.com" };
    rendered.rerender(<InventoryView user={nextUser} onNotice={onNotice} onSessionRequired={vi.fn()} />);
    await act(async () => pending.resolve(movement));
    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).not.toBeNull();
    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${nextUser.id}`)).toBeNull();
    expect(onNotice).not.toHaveBeenCalled();
  });

  it("keeps stale table data on refresh errors and supports retry", async () => {
    renderInventory(); await settleInitial();
    mockedView.mockRejectedValueOnce(new Error("SQLITE private")).mockResolvedValueOnce(inventory);
    fireEvent.change(screen.getByLabelText("Buscar producto"), { target: { value: "nuevo" } });
    await settleInitial();
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("No se pudo consultar");
    expect(alert.textContent).not.toContain("SQLITE");
    expect(screen.getAllByText("Cable").length).toBeGreaterThan(0);
    fireEvent.click(within(alert).getByRole("button", { name: "Reintentar" })); await act(async () => undefined);
    expect(mockedView).toHaveBeenCalled();
  });
});
