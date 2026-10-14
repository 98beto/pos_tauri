import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { getProductInventoryDetail } from "../../api/inventory";
import { deferred } from "../../test/fixtures";
import type { InventoryMovement, ProductInventoryDetail as DetailData } from "../../types/inventory_movement";
import { ProductInventoryDetail } from "./ProductInventoryDetail";

vi.mock("../../api/inventory", () => ({ getProductInventoryDetail: vi.fn() }));
const mockedDetail = vi.mocked(getProductInventoryDetail);
const movement: InventoryMovement = {
  id: 1, product_id: 11, product_name: "Cafe actual", sku: "CAFE", product_name_snapshot: "Cafe anterior", sku_snapshot: "CAFE",
  sale_id: 8, ticket_reference: "POS-00000008", record_version: 2, presentation_id: 101, presentation_name: "Bolsa", presentation_barcode: null,
  base_unit: "gram", presentation_mode: "fixed", presentation_base_quantity: 250, unit_price_cents: 1200, reason: "compra", type: "entrada",
  quantity: 500, entered_quantity: 2, base_quantity: 500, stock_before: 500, stock_after: 1000, movement_cost_cents: 2400,
  inventory_cost_before_cents: 1000, inventory_cost_after_cents: 3400, supplier: "Proveedor", document_reference: "FAC-1", note: "Lote nuevo", operation_token: "token", created_at: "2026-09-01T10:00:00Z", updated_at: "2026-09-01T10:00:00Z",
};
const detail: DetailData = {
  product: { id: 11, name: "Cafe actual", sku: "CAFE", stock: 1000, base_unit: "gram", inventory_cost_cents: 3400, inventory_cost_status: "known", average_cost_cents: 3 },
  last_movement_at: movement.created_at, movements: [movement], page: 1, total: 21, page_size: 20,
};

function renderDetail(props: Partial<React.ComponentProps<typeof ProductInventoryDetail>> = {}) {
  return render(<ProductInventoryDetail productId={11} onBack={vi.fn()} onSessionRequired={vi.fn()} {...props} />);
}
async function settle() { await act(() => vi.advanceTimersByTimeAsync(250)); await act(async () => undefined); }

beforeEach(() => { vi.useFakeTimers(); mockedDetail.mockResolvedValue(detail); });

describe("ProductInventoryDetail", () => {
  it("shows the current header and the complete responsive movement table", async () => {
    renderDetail(); await settle();
    expect(screen.getByRole("heading", { name: "Cafe actual" })).toBeTruthy();
    expect(screen.getByText("1 kg")).toBeTruthy();
    expect(screen.getByText("$34.00")).toBeTruthy();
    expect(screen.getByText("2 Bolsa (0.5 kg base)")).toBeTruthy();
    expect(screen.getByText("Proveedor")).toBeTruthy();
    expect(screen.getByText("Lote nuevo")).toBeTruthy();
    expect(screen.getByText("POS-00000008")).toBeTruthy();
    expect(screen.getByText("$12.00 por Bolsa")).toBeTruthy();
    const region = screen.getByRole("region", { name: "Movimientos del producto desplazables" });
    expect(region.getAttribute("tabindex")).toBe("0");
    expect(region.className).toContain("focus-visible:outline-2");
  });

  it("sends all filters, resets the page and paginates remotely", async () => {
    renderDetail(); await settle();
    fireEvent.click(screen.getByRole("button", { name: "Siguiente" })); await settle();
    expect(mockedDetail).toHaveBeenLastCalledWith(11, expect.objectContaining({ page: 2 }));
    fireEvent.change(screen.getByLabelText("Buscar referencia"), { target: { value: "FAC" } });
    fireEvent.change(screen.getByLabelText("Motivo"), { target: { value: "compra" } });
    fireEvent.change(screen.getByLabelText("Tipo"), { target: { value: "entrada" } });
    fireEvent.change(screen.getByLabelText("Fecha inicial"), { target: { value: "2026-09-01" } });
    fireEvent.change(screen.getByLabelText("Fecha final"), { target: { value: "2026-09-30" } });
    await settle();
    expect(mockedDetail).toHaveBeenLastCalledWith(11, { movement_search: "FAC", reason: "compra", type: "entrada", date_from: "2026-09-01", date_to: "2026-09-30", page: 1 });
  });

  it("ignores out-of-order responses", async () => {
    const first = deferred<DetailData>(); const second = deferred<DetailData>();
    mockedDetail.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    renderDetail(); await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.change(screen.getByLabelText("Buscar referencia"), { target: { value: "nuevo" } }); await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => second.resolve({ ...detail, product: { ...detail.product, name: "Resultado nuevo" } }));
    await act(async () => first.resolve({ ...detail, product: { ...detail.product, name: "Resultado viejo" } }));
    expect(screen.getByText("Resultado nuevo")).toBeTruthy(); expect(screen.queryByText("Resultado viejo")).toBeNull();
  });

  it("preserves previous rows on safe errors and retries", async () => {
    renderDetail(); await settle();
    mockedDetail.mockRejectedValueOnce(new Error("SQLITE /private")).mockResolvedValueOnce(detail);
    fireEvent.change(screen.getByLabelText("Buscar referencia"), { target: { value: "otra" } }); await settle();
    expect(screen.getByText("Se muestran los resultados anteriores; no corresponden a los filtros actuales.")).toBeTruthy();
    expect(screen.getByText("Proveedor")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).not.toContain("SQLITE");
    fireEvent.click(screen.getByRole("button", { name: "Reintentar" })); await act(async () => undefined);
    expect(mockedDetail).toHaveBeenCalled();
  });

  it("delegates expired sessions and does not show a retry error", async () => {
    const onSessionRequired = vi.fn();
    mockedDetail.mockRejectedValue({ category: "authentication", code: "SESSION_REQUIRED", message: "Sesion requerida" });
    renderDetail({ onSessionRequired }); await settle();
    expect(onSessionRequired).toHaveBeenCalledOnce(); expect(screen.queryByRole("alert")).toBeNull();
  });

  it("focuses the detail heading when mounted", async () => {
    renderDetail();
    await act(() => vi.runAllTimersAsync());
    expect(document.activeElement).toBe(screen.getByRole("heading", { name: "Cafe actual" }));
  });

  it("shows the unfiltered empty state", async () => {
    mockedDetail.mockResolvedValueOnce({ ...detail, movements: [], total: 0, last_movement_at: null });
    renderDetail(); await settle();
    expect(screen.getByText("Este producto aun no tiene movimientos")).toBeTruthy();
    expect(screen.getByText("Puedes registrar una compra o ajuste desde Inventario.")).toBeTruthy();
  });

  it("labels an adjustment exit unit cost with its real base unit", async () => {
    mockedDetail.mockResolvedValueOnce({
      ...detail,
      movements: [{
        ...movement,
        id: 2,
        sale_id: null,
        ticket_reference: null,
        presentation_id: null,
        presentation_name: null,
        presentation_mode: null,
        presentation_base_quantity: null,
        reason: "ajuste",
        type: "salida",
        unit_price_cents: 3,
      }],
      total: 1,
    });
    renderDetail(); await settle();
    expect(screen.getByText("$0.03 por gramo")).toBeTruthy();
  });

  it("ignores a response after unmount", async () => {
    const pending = deferred<DetailData>();
    mockedDetail.mockReturnValueOnce(pending.promise);
    const rendered = renderDetail();
    await act(() => vi.advanceTimersByTimeAsync(250));
    rendered.unmount();
    await act(async () => pending.resolve(detail));
    expect(document.querySelector("#product-inventory-title")).toBeNull();
  });
});
