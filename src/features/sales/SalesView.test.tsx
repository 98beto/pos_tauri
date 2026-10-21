import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SalesView } from "./SalesView";
import { getSaleHistoryDetail, getSalesView } from "../../api/sales";
import type { SaleHistoryDetail, SalesView as SalesViewData } from "../../types/sale";

vi.mock("../../api/sales", () => ({
  getSaleHistoryDetail: vi.fn(),
  getSalesView: vi.fn(),
}));

const mockedGetSalesView = vi.mocked(getSalesView);
const mockedGetSaleHistoryDetail = vi.mocked(getSaleHistoryDetail);

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function salesView(): SalesViewData {
  return {
    stats: { sale_count: 2, total_sold: 3000, average_sale: 1500, lines_sold: 3 },
    items: [
      { id: 1, ticket_number: "POS-00000001", sale_date: "2026-01-01", payment_method: "tarjeta", total: 2500, line_count: 2, user_id: 1, user_name: "Admin POS" },
      { id: 2, ticket_number: "POS-00000002", sale_date: "2026-01-02", payment_method: "efectivo", total: 500, line_count: 1, user_id: 1, user_name: "Admin POS" },
    ],
  };
}

function detail(id: number): SaleHistoryDetail {
  return {
    id,
    ticket_number: `POS-${String(id).padStart(8, "0")}`,
    payment_method: "tarjeta",
    sale_date: "2026-01-01",
    user_id: 1,
    user_name: `Cajero ${id}`,
    user_email: `cajero${id}@example.com`,
    lines: [],
    line_count: 0,
    subtotal: id * 100,
    discount_type: "none",
    discount_value: 0,
    discount_amount: 0,
    line_totals_verifiable: true,
    total: id * 100,
    cash_received: null,
    change_amount: null,
  };
}

function detailWithPresentations(): SaleHistoryDetail {
  return {
    ...detail(1),
    line_count: 3,
    subtotal: 12_500,
    discount_type: "percentage",
    discount_value: 1_000,
    discount_amount: 1_250,
    line_totals_verifiable: false,
    total: 11_250,
    lines: [
      {
        product_id: 10,
        presentation_id: 101,
        product_name: "Cafe historico",
        sku: "CAFE-01",
        presentation_name: "Bolsa 250 g",
        presentation_barcode: "7500000000101",
        base_unit: "gram",
        presentation_mode: "fixed",
        presentation_base_quantity: 250,
        quantity: 2,
        catalog_unit_price: 5000,
        entered_quantity: 2,
        base_consumed: 500,
        unit_price: 4500,
        subtotal: 9000,
      },
      {
        product_id: 10,
        presentation_id: 102,
        product_name: "Cafe historico",
        sku: "CAFE-01",
        presentation_name: "Kilogramo",
        presentation_barcode: null,
        base_unit: "gram",
        presentation_mode: "measured",
        presentation_base_quantity: 500,
        quantity: 250,
        catalog_unit_price: 12000,
        entered_quantity: 250,
        base_consumed: 250,
        unit_price: 12000,
        subtotal: 3000,
      },
      {
        product_id: 11,
        presentation_id: null,
        product_name: "Producto legacy",
        sku: "LEGACY-01",
        presentation_name: null,
        presentation_barcode: null,
        base_unit: null,
        presentation_mode: null,
        presentation_base_quantity: null,
        quantity: 1,
        catalog_unit_price: null,
        entered_quantity: 1,
        base_consumed: 1,
        unit_price: 500,
        subtotal: null,
      },
    ],
  };
}

function lengthDetail(): SaleHistoryDetail {
  return {
    ...detail(1),
    line_count: 1,
    subtotal: 1_500,
    discount_type: "fixed",
    discount_value: 150,
    discount_amount: 150,
    total: 1_350,
    lines: [{
      product_id: 40,
      presentation_id: 401,
      product_name: "Cable electrico",
      sku: "CABLE-1",
      presentation_name: "Metro",
      presentation_barcode: "7500000000401",
      base_unit: "centimeter",
      presentation_mode: "measured",
      presentation_base_quantity: 100,
      quantity: 75,
      catalog_unit_price: 2_500,
      unit_price: 2_000,
      entered_quantity: 75,
      base_consumed: 75,
      subtotal: 1_500,
    }],
  };
}

afterEach(() => {
  vi.clearAllMocks();
  vi.useRealTimers();
});

describe("SalesView", () => {
  it("sends ticket and payment filters and replaces statistics with remote results", async () => {
    vi.useFakeTimers();
    mockedGetSalesView.mockResolvedValueOnce(salesView()).mockResolvedValueOnce({
      stats: { sale_count: 1, total_sold: 500, average_sale: 500, lines_sold: 1 },
      items: [salesView().items[1]],
    });
    render(<SalesView onSessionRequired={vi.fn()} />);
    await act(() => vi.advanceTimersByTimeAsync(250));

    fireEvent.change(screen.getByLabelText("Buscar ticket"), { target: { value: "00000002" } });
    fireEvent.change(screen.getByLabelText("Metodo de pago"), { target: { value: "efectivo" } });
    await act(() => vi.advanceTimersByTimeAsync(250));

    expect(mockedGetSalesView).toHaveBeenLastCalledWith({ search: "00000002", payment_method: "efectivo" });
    expect(screen.getByText("POS-00000002")).toBeTruthy();
    expect(screen.getAllByText("Segun filtros y resultados actuales")).toHaveLength(4);
    const region = screen.getByRole("region", { name: "Historial de tickets desplazable" });
    expect(region.className).toContain("focus-visible:outline-2");
  });

  it("does not present zero statistics before an initial success or after an initial error", async () => {
    vi.useFakeTimers();
    const request = deferred<SalesViewData>();
    mockedGetSalesView.mockReturnValue(request.promise);
    render(<SalesView onSessionRequired={vi.fn()} />);

    expect(screen.getAllByText("-")).toHaveLength(4);
    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => request.reject(new Error("offline")));

    expect(screen.getAllByText("-")).toHaveLength(4);
    expect(screen.getByRole("alert").textContent).toContain("No se pudo consultar el historial de ventas");
  });

  it("hides stale statistics while refreshing filtered results", async () => {
    vi.useFakeTimers();
    const initial = deferred<SalesViewData>();
    const filtered = deferred<SalesViewData>();
    mockedGetSalesView.mockReturnValueOnce(initial.promise).mockReturnValueOnce(filtered.promise);
    render(<SalesView onSessionRequired={vi.fn()} />);

    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => initial.resolve(salesView()));
    expect(screen.getByText("Ventas en resultados")).toBeTruthy();
    expect(screen.getAllByText("Segun los resultados actuales")).toHaveLength(4);

    fireEvent.change(screen.getByLabelText("Metodo de pago"), { target: { value: "tarjeta" } });
    expect(screen.getAllByText("-")).toHaveLength(4);
    expect(screen.getByText("Ventas en resultados").parentElement?.parentElement?.getAttribute("aria-busy")).toBe("true");
    expect(screen.getAllByText("Pendiente de actualizacion")).toHaveLength(4);
    expect(screen.getByText("Actualizando; se muestran temporalmente los resultados anteriores.")).toBeTruthy();

    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => filtered.resolve({
      stats: { sale_count: 1, total_sold: 2500, average_sale: 2500, lines_sold: 2 },
      items: [salesView().items[0]],
    }));
    expect(screen.getByText("1")).toBeTruthy();
  });

  it("keeps stale statistics hidden when a filtered refresh fails", async () => {
    vi.useFakeTimers();
    mockedGetSalesView.mockResolvedValueOnce(salesView()).mockRejectedValueOnce(new Error("offline"));
    render(<SalesView onSessionRequired={vi.fn()} />);

    await act(() => vi.advanceTimersByTimeAsync(250));
    expect(screen.getByText("Ventas en resultados")).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Metodo de pago"), { target: { value: "tarjeta" } });
    await act(() => vi.advanceTimersByTimeAsync(250));

    expect(screen.getAllByText("-")).toHaveLength(4);
    expect(screen.getAllByText("No disponible por el error actual")).toHaveLength(4);
    expect(screen.getByText("Se muestran los resultados anteriores; no corresponden a los filtros actuales.")).toBeTruthy();
    expect(screen.getByText("POS-00000001")).toBeTruthy();
  });

  it("never lets an older detail response populate a newer ticket modal", async () => {
    vi.useFakeTimers();
    mockedGetSalesView.mockResolvedValue(salesView());
    const first = deferred<SaleHistoryDetail>();
    const second = deferred<SaleHistoryDetail>();
    mockedGetSaleHistoryDetail.mockImplementation((id) => id === 1 ? first.promise : second.promise);
    render(<SalesView onSessionRequired={vi.fn()} />);

    await act(() => vi.advanceTimersByTimeAsync(250));
    const firstButton = screen.getByRole("button", { name: "Ver detalle de POS-00000001" });
    const secondButton = screen.getByRole("button", { name: "Ver detalle de POS-00000002" });
    fireEvent.click(firstButton);
    fireEvent.click(secondButton);
    expect(mockedGetSaleHistoryDetail).toHaveBeenCalledTimes(2);

    await act(async () => first.resolve(detail(1)));
    expect(screen.queryByText("Cajero 1")).toBeNull();
    await act(async () => second.resolve(detail(2)));
    expect(screen.getByText("Cajero 2")).toBeTruthy();
    expect(screen.getByRole("dialog").textContent).toContain("POS-00000002");
  });

  it("shows each presentation of the same product and preserves legacy line snapshots", async () => {
    vi.useFakeTimers();
    mockedGetSalesView.mockResolvedValue(salesView());
    mockedGetSaleHistoryDetail.mockResolvedValue(detailWithPresentations());
    render(<SalesView onSessionRequired={vi.fn()} />);

    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Ver detalle de POS-00000001" }));
    await act(async () => undefined);

    const dialog = screen.getByRole("dialog");
    const linesRegion = screen.getByRole("region", { name: "Lineas del ticket desplazables" });
    expect(linesRegion.getAttribute("tabindex")).toBe("0");
    expect(linesRegion.className).toContain("focus-visible:outline-2");
    expect(screen.getAllByText("Cafe historico")).toHaveLength(2);
    expect(dialog.textContent).toContain("Bolsa 250 g");
    expect(dialog.textContent).toContain("Kilogramo");
    expect(dialog.textContent).toContain("Base: 0.5 kg");
    expect(dialog.textContent).toContain("0.25 kg");
    expect(dialog.textContent).toContain("$120.00 por 500 g");
    expect(dialog.textContent).toContain("Aplicado: $45.00 por Bolsa 250 g");
    expect(dialog.textContent).toContain("Catalogo: $50.00 por Bolsa 250 g");
    expect(dialog.textContent).not.toMatch(/costo de inventario|costo desconocido/i);
    expect(dialog.textContent).toContain("Sin presentacion registrada");
    expect(dialog.textContent).toContain("No representable");
    expect(dialog.textContent).toContain("Subtotal$125.00");
    expect(dialog.textContent).toContain("Descuento 10%-$12.50");
    expect(dialog.textContent).toContain("Total$112.50");
    expect(dialog.textContent).toContain("las lineas legadas no permiten verificar su suma");
    expect(dialog.textContent).toContain("Cafe historico");
    expect(dialog.textContent).not.toContain("Articulos");
  });

  it("shows centimeter snapshots, custom price, line total, and global discount independently", async () => {
    vi.useFakeTimers();
    mockedGetSalesView.mockResolvedValue(salesView());
    mockedGetSaleHistoryDetail.mockResolvedValue(lengthDetail());
    render(<SalesView onSessionRequired={vi.fn()} />);

    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Ver detalle de POS-00000001" }));
    await act(async () => undefined);

    const dialog = screen.getByRole("dialog");
    expect(dialog.textContent).toContain("75 cm");
    expect(dialog.textContent).toContain("Aplicado: $20.00 por metro");
    expect(dialog.textContent).toContain("Catalogo: $25.00 por metro");
    expect(dialog.textContent).not.toMatch(/costo de inventario|costo desconocido/i);
    expect(dialog.textContent).toContain("$15.00");
    expect(dialog.textContent).toContain("Subtotal$15.00");
    expect(dialog.textContent).toContain("Descuento fijo-$1.50");
    expect(dialog.textContent).toContain("Total$13.50");
  });

  it.each([
    ["null", null],
    ["zero", 0],
    ["negative", -1],
    ["fractional", 1.5],
    ["unsafe integer", Number.MAX_SAFE_INTEGER + 1],
  ])("labels measured history prices whose presentation base is %s as unavailable", async (_case, presentationBaseQuantity) => {
    vi.useFakeTimers();
    const saleDetail = lengthDetail();
    saleDetail.lines[0] = {
      ...saleDetail.lines[0],
      presentation_base_quantity: presentationBaseQuantity,
    };
    mockedGetSalesView.mockResolvedValue(salesView());
    mockedGetSaleHistoryDetail.mockResolvedValue(saleDetail);
    render(<SalesView onSessionRequired={vi.fn()} />);

    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Ver detalle de POS-00000001" }));
    await act(async () => undefined);

    const dialog = screen.getByRole("dialog");
    expect(dialog.textContent).toContain("Aplicado: $20.00 · base no disponible");
    expect(dialog.textContent).toContain("Catalogo: $25.00 · base no disponible");
    expect(dialog.textContent).not.toMatch(/costo de inventario|costo desconocido/i);
    expect(dialog.textContent).toContain("$15.00");
    expect(dialog.textContent).not.toContain("por 0");
  });

  it.each([
    [1, "1 cm"],
    [75, "75 cm"],
    [100, "1 m"],
    [125, "1.25 m"],
  ])("formats a historical length of %i centimeters as %s", async (centimeters, formatted) => {
    vi.useFakeTimers();
    const saleDetail = lengthDetail();
    saleDetail.lines[0] = {
      ...saleDetail.lines[0],
      quantity: centimeters,
      entered_quantity: centimeters,
      base_consumed: centimeters,
    };
    mockedGetSalesView.mockResolvedValue(salesView());
    mockedGetSaleHistoryDetail.mockResolvedValue(saleDetail);
    render(<SalesView onSessionRequired={vi.fn()} />);

    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Ver detalle de POS-00000001" }));
    await act(async () => undefined);

    expect(screen.getByRole("dialog").textContent).toContain(formatted);
  });

  it("ignores out-of-order list responses and completion after unmount", async () => {
    vi.useFakeTimers();
    mockedGetSalesView.mockResolvedValueOnce(salesView());
    const first = deferred<SalesViewData>();
    const second = deferred<SalesViewData>();
    mockedGetSalesView.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { unmount } = render(<SalesView onSessionRequired={vi.fn()} />);
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.change(screen.getByLabelText("Buscar ticket"), { target: { value: "a" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.change(screen.getByLabelText("Buscar ticket"), { target: { value: "b" } });
    await act(() => vi.advanceTimersByTimeAsync(250));

    const resultB = { ...salesView(), items: [{ ...salesView().items[0], ticket_number: "RESULTADO-B" }] };
    const resultA = { ...salesView(), items: [{ ...salesView().items[0], ticket_number: "RESULTADO-A" }] };
    await act(async () => second.resolve(resultB));
    expect(screen.getByText("RESULTADO-B")).toBeTruthy();
    await act(async () => first.resolve(resultA));
    expect(screen.queryByText("RESULTADO-A")).toBeNull();

    const afterUnmount = deferred<SalesViewData>();
    mockedGetSalesView.mockReturnValueOnce(afterUnmount.promise);
    fireEvent.change(screen.getByLabelText("Buscar ticket"), { target: { value: "c" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    unmount();
    await act(async () => afterUnmount.resolve(salesView()));
  });

  it("does not expose technical text from an unknown detail failure", async () => {
    vi.useFakeTimers();
    mockedGetSalesView.mockResolvedValue(salesView());
    mockedGetSaleHistoryDetail.mockRejectedValue(new Error("SQLITE_CORRUPT /var/data.db"));
    render(<SalesView onSessionRequired={vi.fn()} />);
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Ver detalle de POS-00000001" }));
    await act(async () => undefined);

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("No se pudo cargar el detalle de la venta.");
    expect(alert.textContent).not.toContain("SQLITE_CORRUPT");
  });
});
