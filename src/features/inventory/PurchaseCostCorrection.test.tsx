import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { confirmPurchasePriceCorrection, quotePurchasePriceCorrection } from "../../api/inventory";
import { deferred } from "../../test/fixtures";
import type { InventoryMovement, PurchasePriceCorrectionQuote } from "../../types/inventory_movement";
import { PurchaseCostCorrection } from "./PurchaseCostCorrection";

vi.mock("../../api/inventory", () => ({ confirmPurchasePriceCorrection: vi.fn(), quotePurchasePriceCorrection: vi.fn() }));
const movement: InventoryMovement = {
  id: 7, product_id: 11, product_name: "Cable", sku: "CABLE", product_name_snapshot: "Cable historico", sku_snapshot: "OLD", sale_id: null, ticket_reference: null,
  record_version: 3, origin: "manual", presentation_id: 101, presentation_name: "Metro", presentation_barcode: null, base_unit: "centimeter", presentation_mode: "measured", presentation_base_quantity: 100,
  purchase_unit_price_cents: 2500, purchase_total_cents: 1875, reason: "compra", type: "entrada", quantity: 75, entered_quantity: 75, base_quantity: 75, stock_before: 100, stock_after: 175,
  supplier: "Proveedor", document_reference: "FAC-1", note: "Lote", operation_token: "token", created_at: "2026-10-01T10:00:00Z", updated_at: "2026-10-01T10:00:00Z",
};
const quote: PurchasePriceCorrectionQuote = { movement_id: 7, product_id: 11, product_name: "Cable", product_sku: "CABLE", base_unit: "centimeter", capture_name: "Metro", presentation_id: 101, presentation_name: "Metro", presentation_mode: "measured", presentation_base_quantity: 100, entered_quantity: 75, base_quantity: 75, previous_purchase_unit_price_cents: 2500, new_purchase_unit_price_cents: 3000, previous_purchase_total_cents: 1875, new_purchase_total_cents: 2250, quote_fingerprint: "correction:v1" };
const mockedQuote = vi.mocked(quotePurchasePriceCorrection); const mockedConfirm = vi.mocked(confirmPurchasePriceCorrection);
async function submit() { fireEvent.submit(document.querySelector("form")!); await act(async () => undefined); }

beforeEach(() => { mockedQuote.mockReset().mockResolvedValue(quote); mockedConfirm.mockReset().mockResolvedValue(quote); });

describe("PurchaseCostCorrection", () => {
  it("only permits complete manual v3 purchases", () => {
    const { rerender } = render(<PurchaseCostCorrection movement={movement} onApplied={vi.fn()} onSessionRequired={vi.fn()} />);
    expect(screen.getByRole("button", { name: /Corregir precio de compra/ })).toBeTruthy();
    for (const change of [{ record_version: 2 }, { reason: "ajuste" as const }, { origin: "sale" as const }, { purchase_unit_price_cents: null }, { purchase_total_cents: null }]) {
      rerender(<PurchaseCostCorrection movement={{ ...movement, ...change }} onApplied={vi.fn()} onSessionRequired={vi.fn()} />); expect(screen.queryByRole("button", { name: /Corregir precio/ })).toBeNull();
    }
  });

  it("quotes and confirms only the purchase price and amount with current contract names", async () => {
    const onApplied = vi.fn(); render(<PurchaseCostCorrection movement={movement} onApplied={onApplied} onSessionRequired={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /Corregir precio/ })); fireEvent.change(screen.getByLabelText(/Nuevo precio de compra/), { target: { value: "30.00" } }); await submit();
    expect(mockedQuote).toHaveBeenCalledWith({ movement_id: 7, new_purchase_unit_price: "30.00" }); expect(screen.getByText("Resumen por confirmar")).toBeTruthy(); expect(screen.getByText(/No se recalculan ventas/)).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/historial recalculado|movimientos afectados|costo actual del producto/i);
    await submit(); expect(mockedConfirm).toHaveBeenCalledWith({ movement_id: 7, new_purchase_unit_price: "30.00", quote_fingerprint: "correction:v1" }); expect(onApplied).toHaveBeenCalledOnce(); expect(screen.getByRole("status").textContent).toContain("No se modificaron stock");
  });

  it("requires a new quote after stale state", async () => {
    mockedConfirm.mockRejectedValueOnce({ category: "conflict", code: "PURCHASE_PRICE_CORRECTION_CHANGED", message: "stale" });
    render(<PurchaseCostCorrection movement={movement} onApplied={vi.fn()} onSessionRequired={vi.fn()} />); fireEvent.click(screen.getByRole("button", { name: /Corregir precio/ })); fireEvent.change(screen.getByLabelText(/Nuevo precio de compra/), { target: { value: "30" } }); await submit(); await submit();
    expect(screen.getByRole("alert").textContent).toContain("Calcula nuevamente"); expect(screen.getByRole("button", { name: "Calcular importe" })).toBeTruthy();
  });

  it("does not discard the quote for unrelated conflict codes", async () => {
    mockedConfirm.mockRejectedValueOnce({ category: "conflict", code: "QUOTE_CHANGED", message: "otro conflicto" });
    render(<PurchaseCostCorrection movement={movement} onApplied={vi.fn()} onSessionRequired={vi.fn()} />); fireEvent.click(screen.getByRole("button", { name: /Corregir precio/ })); fireEvent.change(screen.getByLabelText(/Nuevo precio de compra/), { target: { value: "30" } }); await submit(); await submit();
    expect(screen.getByRole("alert").textContent).toContain("otro conflicto"); expect(screen.getByRole("button", { name: "Confirmar correccion" })).toBeTruthy();
  });

  it("keeps the quote for an idempotent retry after an unknown confirmation failure", async () => {
    mockedConfirm.mockRejectedValueOnce(new Error("transport")).mockResolvedValueOnce(quote);
    render(<PurchaseCostCorrection movement={movement} onApplied={vi.fn()} onSessionRequired={vi.fn()} />); fireEvent.click(screen.getByRole("button", { name: /Corregir precio/ })); fireEvent.change(screen.getByLabelText(/Nuevo precio de compra/), { target: { value: "30" } }); await submit(); await submit(); expect(screen.getByRole("alert").textContent).toContain("misma cotizacion"); await submit();
    expect(mockedConfirm).toHaveBeenCalledTimes(2); expect(mockedConfirm.mock.calls[0]).toEqual(mockedConfirm.mock.calls[1]);
  });

  it("routes price validation errors to the field", async () => {
    mockedQuote.mockRejectedValueOnce({ category: "validation", code: "SAME_PRICE", field: "new_purchase_unit_price", message: "El precio debe cambiar" });
    render(<PurchaseCostCorrection movement={movement} onApplied={vi.fn()} onSessionRequired={vi.fn()} />); fireEvent.click(screen.getByRole("button", { name: /Corregir precio/ })); await submit(); expect(screen.getByText("El precio debe cambiar")).toBeTruthy(); expect(mockedConfirm).not.toHaveBeenCalled();
  });

  it("drops a correction response after unmount", async () => {
    const pending = deferred<Awaited<ReturnType<typeof confirmPurchasePriceCorrection>>>();
    mockedConfirm.mockReturnValueOnce(pending.promise);
    const onApplied = vi.fn(); const onSessionRequired = vi.fn();
    const rendered = render(<PurchaseCostCorrection movement={movement} onApplied={onApplied} onSessionRequired={onSessionRequired} />);
    fireEvent.click(screen.getByRole("button", { name: /Corregir precio/ })); fireEvent.change(screen.getByLabelText(/Nuevo precio de compra/), { target: { value: "30" } }); await submit();
    fireEvent.submit(document.querySelector("form")!); rendered.unmount();
    await act(async () => pending.reject({ category: "authentication", code: "SESSION_REQUIRED", message: "Sesion vencida" }));
    expect(onApplied).not.toHaveBeenCalled(); expect(onSessionRequired).not.toHaveBeenCalled();
  });

  it("drops a successful correction response from a previous movement", async () => {
    const pending = deferred<Awaited<ReturnType<typeof confirmPurchasePriceCorrection>>>();
    mockedConfirm.mockReturnValueOnce(pending.promise);
    const onApplied = vi.fn(); const onSessionRequired = vi.fn();
    const rendered = render(<PurchaseCostCorrection movement={movement} onApplied={onApplied} onSessionRequired={onSessionRequired} />);
    fireEvent.click(screen.getByRole("button", { name: /Corregir precio/ })); fireEvent.change(screen.getByLabelText(/Nuevo precio de compra/), { target: { value: "30" } }); await submit();
    fireEvent.submit(document.querySelector("form")!);
    rendered.rerender(<PurchaseCostCorrection movement={{ ...movement, id: 8 }} onApplied={vi.fn()} onSessionRequired={vi.fn()} />);
    await act(async () => pending.resolve(quote));
    expect(onApplied).not.toHaveBeenCalled(); expect(onSessionRequired).not.toHaveBeenCalled();
    expect(screen.queryByRole("status")).toBeNull();
  });
});
