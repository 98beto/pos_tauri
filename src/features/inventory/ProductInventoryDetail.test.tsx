import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { confirmPurchasePriceCorrection, getProductInventoryDetail, quotePurchasePriceCorrection } from "../../api/inventory";
import { deferred } from "../../test/fixtures";
import type { InventoryMovement, ProductInventoryDetail as DetailData, PurchasePriceCorrectionQuote } from "../../types/inventory_movement";
import { ProductInventoryDetail } from "./ProductInventoryDetail";

vi.mock("../../api/inventory", () => ({ getProductInventoryDetail: vi.fn(), quotePurchasePriceCorrection: vi.fn(), confirmPurchasePriceCorrection: vi.fn() }));
const mockedDetail = vi.mocked(getProductInventoryDetail);
const mockedQuote = vi.mocked(quotePurchasePriceCorrection);
const mockedConfirm = vi.mocked(confirmPurchasePriceCorrection);
const purchase: InventoryMovement = {
  id: 1, product_id: 11, product_name: "Cable", sku: "CABLE", product_name_snapshot: "Cable historico", sku_snapshot: "OLD", sale_id: null, ticket_reference: null,
  record_version: 3, origin: "manual", presentation_id: 101, presentation_name: "Metro", presentation_barcode: null, base_unit: "centimeter", presentation_mode: "measured", presentation_base_quantity: 100,
  purchase_unit_price_cents: 2500, purchase_total_cents: 1875, reason: "compra", type: "entrada", quantity: 75, entered_quantity: 75, base_quantity: 75, stock_before: 100, stock_after: 175,
  supplier: "Proveedor", document_reference: "FAC-1", note: "Lote", operation_token: "token", created_at: "2026-10-01T10:00:00Z", updated_at: "2026-10-01T10:00:00Z",
};
const adjustment: InventoryMovement = { ...purchase, id: 2, reason: "ajuste", type: "salida", presentation_id: null, presentation_name: null, presentation_mode: null, presentation_base_quantity: null, purchase_unit_price_cents: null, purchase_total_cents: null, stock_before: 175, stock_after: 150, base_quantity: 25, note: "Merma" };
const sale: InventoryMovement = { ...adjustment, id: 3, reason: "venta", origin: "sale", ticket_reference: "T-9", note: null };
const detail: DetailData = { product: { id: 11, name: "Cable", sku: "CABLE", stock: 150, base_unit: "centimeter" }, last_movement_at: sale.created_at, movements: [purchase, adjustment, sale], page: 1, total: 21, page_size: 20 };
const correctionQuote: PurchasePriceCorrectionQuote = { movement_id: 1, product_id: 11, product_name: "Cable", product_sku: "CABLE", base_unit: "centimeter", capture_name: "Metro", presentation_id: 101, presentation_name: "Metro", presentation_mode: "measured", presentation_base_quantity: 100, entered_quantity: 75, base_quantity: 75, previous_purchase_unit_price_cents: 2500, new_purchase_unit_price_cents: 3000, previous_purchase_total_cents: 1875, new_purchase_total_cents: 2250, quote_fingerprint: "correction:v1" };
async function settle() { await act(() => vi.advanceTimersByTimeAsync(250)); await act(async () => undefined); }

beforeEach(() => {
  vi.useFakeTimers();
  mockedDetail.mockReset().mockResolvedValue(detail);
  mockedQuote.mockReset().mockResolvedValue(correctionQuote);
  mockedConfirm.mockReset().mockResolvedValue(correctionQuote);
});

describe("ProductInventoryDetail", () => {
  it("blocks both actions while a manual movement is pending", async () => {
    const onPurchase = vi.fn(); const onStockCount = vi.fn();
    render(<ProductInventoryDetail productId={11} onBack={vi.fn()} onSessionRequired={vi.fn()} onPurchase={onPurchase} onStockCount={onStockCount} actionsDisabled />); await settle();
    for (const name of ["Registrar compra", "Ajustar existencias"]) {
      const button = screen.getByRole("button", { name }) as HTMLButtonElement;
      expect(button.disabled).toBe(true); fireEvent.click(button);
    }
    expect(onPurchase).not.toHaveBeenCalled(); expect(onStockCount).not.toHaveBeenCalled();
  });
  it("shows stock and both preselected product actions without valuation", async () => {
    const onPurchase = vi.fn(); const onStockCount = vi.fn();
    render(<ProductInventoryDetail productId={11} onBack={vi.fn()} onSessionRequired={vi.fn()} onPurchase={onPurchase} onStockCount={onStockCount} />); await settle();
    expect(screen.getAllByText("1.5 m").length).toBeGreaterThan(0); expect(document.body.textContent).not.toMatch(/costo promedio|valor a costo/i);
    fireEvent.click(screen.getByRole("button", { name: "Registrar compra" })); fireEvent.click(screen.getByRole("button", { name: "Ajustar existencias" }));
    expect(onPurchase).toHaveBeenCalledOnce(); expect(onStockCount).toHaveBeenCalledOnce();
  });

  it("distinguishes purchase, count and sale documentary fields", async () => {
    render(<ProductInventoryDetail productId={11} onBack={vi.fn()} onSessionRequired={vi.fn()} />); await settle();
    const history = screen.getByRole("heading", { name: "Movimientos del producto" }).closest("section")!;
    const cards = within(history).getAllByRole("article");
    expect(within(cards[0]).getByText("Compra")).toBeTruthy(); expect(cards[0].textContent).toContain("$25.00 por 1 m"); expect(cards[0].textContent).toContain("$18.75"); expect(cards[0].textContent).toContain("Proveedor · FAC-1");
    expect(within(cards[1]).getByText("Ajuste de existencias")).toBeTruthy(); expect(cards[1].textContent).toContain("Stock contado"); expect(cards[1].textContent).toContain("Merma"); expect(cards[1].textContent).not.toContain("Proveedor");
    expect(within(cards[2]).getByText("Venta")).toBeTruthy(); expect(cards[2].textContent).toContain("T-9");
  });

  it("keeps legacy purchases readable but does not offer correction", async () => {
    mockedDetail.mockResolvedValueOnce({ ...detail, movements: [{ ...purchase, record_version: 2, purchase_unit_price_cents: null, purchase_total_cents: null }] });
    render(<ProductInventoryDetail productId={11} onBack={vi.fn()} onSessionRequired={vi.fn()} />); await settle();
    expect(screen.getByText("Compra historica sin precio")).toBeTruthy(); expect(screen.getByText("No disponible en historia")).toBeTruthy(); expect(screen.queryByRole("button", { name: /Corregir precio/ })).toBeNull();
  });

  it("sends filters and resets pagination", async () => {
    render(<ProductInventoryDetail productId={11} onBack={vi.fn()} onSessionRequired={vi.fn()} />); await settle();
    fireEvent.click(screen.getByRole("button", { name: "Siguiente" })); await settle();
    fireEvent.change(screen.getByLabelText("Buscar referencia"), { target: { value: "FAC" } }); fireEvent.change(screen.getByLabelText("Motivo"), { target: { value: "compra" } }); fireEvent.change(screen.getByLabelText("Direccion"), { target: { value: "entrada" } }); await settle();
    expect(mockedDetail).toHaveBeenLastCalledWith(11, expect.objectContaining({ movement_search: "FAC", reason: "compra", type: "entrada", page: 1 }));
  });

  it("retains prior cards on a safe refresh error and retries", async () => {
    render(<ProductInventoryDetail productId={11} onBack={vi.fn()} onSessionRequired={vi.fn()} />); await settle();
    mockedDetail.mockRejectedValueOnce(new Error("private sqlite")); fireEvent.change(screen.getByLabelText("Buscar referencia"), { target: { value: "otra" } }); await settle();
    expect(screen.getByText(/resultados anteriores/)).toBeTruthy(); expect(screen.queryByRole("navigation", { name: "Paginacion del detalle" })).toBeNull(); const history = screen.getByRole("heading", { name: "Movimientos del producto" }).closest("section")!; expect(within(history).getAllByRole("article")).toHaveLength(3); fireEvent.click(screen.getByRole("button", { name: "Reintentar" })); await act(async () => undefined);
    expect(mockedDetail).toHaveBeenCalled();
  });

  it("marks retained results busy and hides stale pagination while refreshing", async () => {
    const refresh = deferred<DetailData>();
    render(<ProductInventoryDetail productId={11} onBack={vi.fn()} onSessionRequired={vi.fn()} />); await settle();
    mockedDetail.mockReturnValueOnce(refresh.promise); fireEvent.change(screen.getByLabelText("Buscar referencia"), { target: { value: "pendiente" } }); await act(() => vi.advanceTimersByTimeAsync(250));
    const history = screen.getByRole("heading", { name: "Movimientos del producto" }).closest("section")!;
    expect(history.getAttribute("aria-busy")).toBe("true"); expect(screen.getByRole("status").textContent).toContain("resultados anteriores"); expect(screen.queryByRole("navigation", { name: "Paginacion del detalle" })).toBeNull();
    await act(async () => refresh.resolve(detail));
  });

  it("marks filters busy before debounce and never requests the previous page", async () => {
    render(<ProductInventoryDetail productId={11} onBack={vi.fn()} onSessionRequired={vi.fn()} />); await settle();
    fireEvent.click(screen.getByRole("button", { name: "Siguiente" })); await settle();
    expect(mockedDetail).toHaveBeenLastCalledWith(11, expect.objectContaining({ page: 2 }));
    const calls = mockedDetail.mock.calls.length;
    fireEvent.change(screen.getByLabelText("Buscar referencia"), { target: { value: "FAC-2" } });
    const history = screen.getByRole("heading", { name: "Movimientos del producto" }).closest("section")!;
    expect(history.getAttribute("aria-busy")).toBe("true"); expect(screen.queryByRole("navigation", { name: "Paginacion del detalle" })).toBeNull();
    await act(() => vi.advanceTimersByTimeAsync(249)); expect(mockedDetail).toHaveBeenCalledTimes(calls);
    await act(() => vi.advanceTimersByTimeAsync(1)); await act(async () => undefined);
    expect(mockedDetail).toHaveBeenLastCalledWith(11, expect.objectContaining({ movement_search: "FAC-2", page: 1 }));
    expect(mockedDetail.mock.calls.slice(calls).some(([, input]) => input?.movement_search === "FAC-2" && input.page !== 1)).toBe(false);
  });

  it("keeps correction success when the applied refresh recreates its callback", async () => {
    const refresh = deferred<DetailData>();
    render(<ProductInventoryDetail productId={11} onBack={vi.fn()} onSessionRequired={vi.fn()} />); await settle();
    mockedDetail.mockReturnValueOnce(refresh.promise);
    fireEvent.click(screen.getByRole("button", { name: /Corregir precio/ }));
    fireEvent.change(screen.getByLabelText(/Nuevo precio de compra/), { target: { value: "30" } });
    fireEvent.submit(document.querySelector("form")!); await act(async () => undefined);
    fireEvent.submit(document.querySelector("form")!); await act(async () => undefined);
    expect(screen.getByRole("status", { name: "" }).textContent).toContain("fueron actualizados");
    await act(async () => refresh.resolve(detail));
    expect(screen.getByRole("status").textContent).toContain("fueron actualizados");
  });
});
