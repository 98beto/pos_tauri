import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { getProductInventoryDetail } from "../../api/inventory";
import { deferred } from "../../test/fixtures";
import type { InventoryMovement, ProductInventoryDetail as DetailData } from "../../types/inventory_movement";
import { ProductInventoryDetail } from "./ProductInventoryDetail";

vi.mock("../../api/inventory", () => ({ getProductInventoryDetail: vi.fn() }));
const mockedDetail = vi.mocked(getProductInventoryDetail);
const movement: InventoryMovement = {
  id: 1, product_id: 11, product_name: "Cable actual", sku: "CABLE", product_name_snapshot: "Cable anterior", sku_snapshot: "CABLE",
  sale_id: null, ticket_reference: null, record_version: 2, origin: "manual", presentation_id: 101, presentation_name: "Metro", presentation_barcode: null,
  base_unit: "centimeter", presentation_mode: "measured", presentation_base_quantity: 100, unit_price_cents: 2500,
  reason: "compra", type: "entrada", quantity: 75, entered_quantity: 75, base_quantity: 75, stock_before: 100, stock_after: 175,
  movement_cost_cents: 1875, inventory_cost_before_cents: 2500, inventory_cost_after_cents: 4375,
  supplier: "Proveedor", document_reference: "FAC-1", note: "Lote nuevo", operation_token: "token",
  created_at: "2026-09-01T10:00:00Z", updated_at: "2026-09-01T10:00:00Z",
};
const detail: DetailData = {
  product: { id: 11, name: "Cable actual", sku: "CABLE", stock: 175, base_unit: "centimeter", inventory_cost_cents: 4375, inventory_cost_status: "known", average_cost_cents: 25 },
  last_movement_at: movement.created_at, movements: [movement], page: 1, total: 21, page_size: 20,
};

function renderDetail(props: Partial<React.ComponentProps<typeof ProductInventoryDetail>> = {}) {
  return render(<ProductInventoryDetail productId={11} onBack={vi.fn()} onSessionRequired={vi.fn()} {...props} />);
}
async function settle() { await act(() => vi.advanceTimersByTimeAsync(250)); await act(async () => undefined); }

beforeEach(() => { vi.useFakeTimers(); mockedDetail.mockReset().mockResolvedValue(detail); });

describe("ProductInventoryDetail", () => {
  it("shows centimeters and inventory cost values", async () => {
    renderDetail(); await settle();
    expect(screen.getByRole("heading", { name: "Cable actual" })).toBeTruthy();
    expect(screen.getByText("1.75 m")).toBeTruthy();
    expect(screen.getAllByText("$43.75").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("75 cm")).toBeTruthy();
    expect(screen.getByText("$25.00 por 1 m")).toBeTruthy();
    expect(screen.getByText("$18.75")).toBeTruthy();
    expect(screen.getByText("Valor a costo")).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/importe a precio cliente|valor potencial/i);
  });

  it("uses default presentation snapshots as the price basis for a v3 direct capture", async () => {
    mockedDetail.mockResolvedValueOnce({
      ...detail,
      movements: [{ ...movement, record_version: 2, presentation_id: null, presentation_name: "Metro predeterminado", presentation_mode: "measured", presentation_base_quantity: 100, unit_price_cents: 2500 }],
    });
    renderDetail(); await settle();
    const row = screen.getByText("Metro predeterminado").closest("tr")!;
    expect(row.textContent).toContain("$25.00 por 1 m");
  });

  it("renders null historical price and amount as dashes", async () => {
    mockedDetail.mockResolvedValueOnce({ ...detail, movements: [{ ...movement, record_version: 2, unit_price_cents: null, movement_cost_cents: null }] });
    renderDetail(); await settle();
    const row = screen.getByText("Proveedor").closest("tr")!;
    expect(row.textContent?.match(/-/g)?.length).toBeGreaterThanOrEqual(2);
    expect(row.textContent).not.toContain("Sin costo");
  });

  it("still renders a historical fixed non-piece snapshot", async () => {
    mockedDetail.mockResolvedValueOnce({ ...detail, movements: [{ ...movement, record_version: 2, presentation_name: "Rollo antiguo", presentation_mode: "fixed", presentation_base_quantity: 250, entered_quantity: 2, base_quantity: 500 }] });
    renderDetail(); await settle();
    expect(screen.getByText("2 Rollo antiguo (5 m base)")).toBeTruthy();
  });

  it("uses measured, fixed and direct snapshots for adjustment exit prices", async () => {
    mockedDetail.mockResolvedValueOnce({
      ...detail,
      movements: [
        { ...movement, id: 2, reason: "ajuste", type: "salida", presentation_name: "Metro", presentation_mode: "measured", presentation_base_quantity: 100 },
        { ...movement, id: 3, reason: "ajuste", type: "salida", presentation_name: "Rollo", presentation_mode: "fixed", presentation_base_quantity: 250 },
        { ...movement, id: 4, reason: "ajuste", type: "salida", presentation_id: null, presentation_name: null, presentation_mode: null, presentation_base_quantity: null },
      ],
    });
    renderDetail(); await settle();
    expect(screen.getByText("$25.00 por 1 m")).toBeTruthy();
    expect(screen.getByText("$25.00 por Rollo")).toBeTruthy();
    expect(screen.getByText("$25.00 por centimetro")).toBeTruthy();
  });

  it("sends all filters, resets the page and paginates remotely", async () => {
    renderDetail(); await settle();
    fireEvent.click(screen.getByRole("button", { name: "Siguiente" })); await settle();
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
    expect(screen.getByText("Resultado nuevo")).toBeTruthy();
    expect(screen.queryByText("Resultado viejo")).toBeNull();
  });

  it("preserves previous rows on safe errors and retries", async () => {
    renderDetail(); await settle();
    mockedDetail.mockRejectedValueOnce(new Error("SQLITE /private")).mockResolvedValueOnce(detail);
    fireEvent.change(screen.getByLabelText("Buscar referencia"), { target: { value: "otra" } }); await settle();
    expect(screen.getByText("Se muestran los resultados anteriores; no corresponden a los filtros actuales.")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).not.toContain("SQLITE");
    fireEvent.click(screen.getByRole("button", { name: "Reintentar" })); await act(async () => undefined);
    expect(mockedDetail).toHaveBeenCalled();
  });
});
