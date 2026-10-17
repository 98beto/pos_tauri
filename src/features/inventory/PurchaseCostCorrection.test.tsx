import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { confirmPurchaseCostCorrection, quotePurchaseCostCorrection } from "../../api/inventory";
import type { InventoryMovement, PurchaseCostCorrectionQuote } from "../../types/inventory_movement";
import { PurchaseCostCorrection } from "./PurchaseCostCorrection";

vi.mock("../../api/inventory", () => ({
  confirmPurchaseCostCorrection: vi.fn(),
  quotePurchaseCostCorrection: vi.fn(),
}));

const movement: InventoryMovement = {
  id: 7, product_id: 11, product_name: "Cable", sku: "CABLE", product_name_snapshot: "Cable historico", sku_snapshot: "CABLE",
  sale_id: null, ticket_reference: null, record_version: 2, origin: "manual", presentation_id: 101, presentation_name: "Metro", presentation_barcode: null,
  base_unit: "centimeter", presentation_mode: "measured", presentation_base_quantity: 100, unit_price_cents: 2500,
  reason: "compra", type: "entrada", quantity: 75, entered_quantity: 75, base_quantity: 75, stock_before: 100, stock_after: 175,
  movement_cost_cents: 1875, inventory_cost_before_cents: 2500, inventory_cost_after_cents: 4375,
  supplier: "Proveedor", document_reference: "FAC-1", note: "Lote", operation_token: "token",
  created_at: "2026-10-01T10:00:00Z", updated_at: "2026-10-01T10:00:00Z",
};

const quote: PurchaseCostCorrectionQuote = {
  movement_id: 7, product_id: 11, product_name: "Cable", product_sku: "CABLE", base_unit: "centimeter", capture_name: "Metro",
  presentation_id: 101, presentation_name: "Metro", presentation_mode: "measured", presentation_base_quantity: 100,
  entered_quantity: 75, base_quantity: 75, previous_unit_price_cents: 2500, new_unit_price_cents: 3000,
  previous_movement_cost_cents: 1875, new_movement_cost_cents: 2250, current_inventory_cost_cents: 4375,
  corrected_inventory_cost_cents: 4750, affected_movement_count: 3, affected_sale_count: 1,
  first_affected_at: movement.created_at, last_affected_at: "2026-10-02T10:00:00Z", quote_fingerprint: "purchase-cost-correction-v1:test",
};

const mockedQuote = vi.mocked(quotePurchaseCostCorrection);
const mockedConfirm = vi.mocked(confirmPurchaseCostCorrection);

async function submit() {
  fireEvent.submit(document.querySelector("form")!);
  await act(async () => undefined);
}

beforeEach(() => {
  mockedQuote.mockReset().mockResolvedValue(quote);
  mockedConfirm.mockReset().mockResolvedValue(quote);
});

describe("PurchaseCostCorrection", () => {
  it("only offers the action for complete manual purchase v2 rows", () => {
    const { rerender } = render(<PurchaseCostCorrection movement={movement} onApplied={vi.fn()} onSessionRequired={vi.fn()} />);
    expect(screen.getByRole("button", { name: /Corregir precio de compra/ })).toBeTruthy();
    for (const ineligible of [
      { origin: "sale" as const }, { reason: "ajuste" as const }, { type: "salida" as const }, { record_version: 1 }, { movement_cost_cents: null },
    ]) {
      rerender(<PurchaseCostCorrection movement={{ ...movement, ...ineligible }} onApplied={vi.fn()} onSessionRequired={vi.fn()} />);
      expect(screen.queryByRole("button", { name: /Corregir precio de compra/ })).toBeNull();
    }
  });

  it("quotes the full impact, warns, confirms, refreshes and restores focus", async () => {
    vi.spyOn(HTMLElement.prototype, "getClientRects").mockReturnValue({ length: 1 } as DOMRectList);
    const onApplied = vi.fn();
    render(<PurchaseCostCorrection movement={movement} onApplied={onApplied} onSessionRequired={vi.fn()} />);
    const trigger = screen.getByRole("button", { name: /Corregir precio de compra/ });
    trigger.focus();
    fireEvent.click(trigger);
    await act(async () => undefined);
    fireEvent.change(screen.getByLabelText(/Nuevo precio de compra/), { target: { value: "30.00" } });
    await submit();
    expect(mockedQuote).toHaveBeenCalledWith({ movement_id: 7, new_unit_price: "30.00" });
    expect(screen.getByText("Impacto cotizado")).toBeTruthy();
    expect(screen.getByText(/3 movimientos · 1 ventas/)).toBeTruthy();
    expect(screen.getByText(/Reemplazo irreversible/)).toBeTruthy();
    await submit();
    expect(mockedConfirm).toHaveBeenCalledWith({ movement_id: 7, new_unit_price: "30.00", quote_fingerprint: quote.quote_fingerprint });
    expect(onApplied).toHaveBeenCalledOnce();
    expect(screen.getByRole("status").textContent).toContain("ya fue aplicada");
    fireEvent.click(screen.getByRole("button", { name: "Cerrar" }));
    expect(document.activeElement).toBe(trigger);
  });

  it.each([
    { label: "centimeter measured", quoted: quote, expected: "Captura original: 75 cm; movimiento al inventario: 75 cm." },
    {
      label: "fixed pieces",
      quoted: { ...quote, base_unit: "piece" as const, capture_name: "Caja de 12", presentation_name: "Caja de 12", presentation_mode: "fixed" as const, presentation_base_quantity: 12, entered_quantity: 2, base_quantity: 24 },
      expected: "Captura original: 2 Caja de 12; movimiento al inventario: 24 piezas.",
    },
  ])("formats the $label quantity identity", async ({ quoted, expected }) => {
    mockedQuote.mockResolvedValueOnce(quoted);
    render(<PurchaseCostCorrection movement={movement} onApplied={vi.fn()} onSessionRequired={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /Corregir precio de compra/ }));
    fireEvent.change(screen.getByLabelText(/Nuevo precio de compra/), { target: { value: "30.00" } });
    await submit();

    expect(screen.getByText(expected)).toBeTruthy();
  });

  it("forces a new quote after stale state", async () => {
    mockedConfirm.mockRejectedValueOnce({ category: "conflict", code: "PURCHASE_COST_CORRECTION_STALE", message: "stale" });
    render(<PurchaseCostCorrection movement={movement} onApplied={vi.fn()} onSessionRequired={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /Corregir precio de compra/ }));
    fireEvent.change(screen.getByLabelText(/Nuevo precio de compra/), { target: { value: "30.00" } });
    await submit();
    await submit();
    expect(screen.getByRole("alert").textContent).toContain("Calcula nuevamente");
    expect(screen.getByRole("button", { name: "Calcular impacto" })).toBeTruthy();
  });

  it("shows the same-price quote error on the price field", async () => {
    mockedQuote.mockRejectedValueOnce({
      category: "validation",
      code: "PURCHASE_COST_CORRECTION_SAME_PRICE",
      message: "El nuevo precio debe ser diferente al precio de compra actual",
      field: "new_unit_price",
    });
    render(<PurchaseCostCorrection movement={movement} onApplied={vi.fn()} onSessionRequired={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /Corregir precio de compra/ }));
    fireEvent.change(screen.getByLabelText(/Nuevo precio de compra/), { target: { value: "25.00" } });
    await submit();

    expect(screen.getByText("El nuevo precio debe ser diferente al precio de compra actual")).toBeTruthy();
    expect(mockedConfirm).not.toHaveBeenCalled();
  });

  it("keeps the same quote for a safe confirm retry after an unknown response failure", async () => {
    mockedConfirm.mockRejectedValueOnce(new Error("transport")).mockResolvedValueOnce(quote);
    render(<PurchaseCostCorrection movement={movement} onApplied={vi.fn()} onSessionRequired={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /Corregir precio de compra/ }));
    fireEvent.change(screen.getByLabelText(/Nuevo precio de compra/), { target: { value: "30.00" } });
    await submit();
    await submit();
    expect(screen.getByRole("alert").textContent).toContain("reintentar de forma segura");
    await submit();
    expect(mockedConfirm).toHaveBeenCalledTimes(2);
    expect(mockedConfirm.mock.calls[0]).toEqual(mockedConfirm.mock.calls[1]);
  });
});
