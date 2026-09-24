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
    stats: { sale_count: 2, total_sold: 3000, average_sale: 1500, items_sold: 3 },
    items: [
      { id: 1, ticket_number: "POS-00000001", sale_date: "2026-01-01", payment_method: "tarjeta", total: 2500, item_count: 2, user_id: 1, user_name: "Admin POS" },
      { id: 2, ticket_number: "POS-00000002", sale_date: "2026-01-02", payment_method: "efectivo", total: 500, item_count: 1, user_id: 1, user_name: "Admin POS" },
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
    item_count: 0,
    total: id * 100,
    cash_received: null,
    change_amount: null,
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
      stats: { sale_count: 1, total_sold: 500, average_sale: 500, items_sold: 1 },
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
      stats: { sale_count: 1, total_sold: 2500, average_sale: 2500, items_sold: 2 },
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
    const buttons = screen.getAllByRole("button", { name: "Ver detalle" });
    fireEvent.click(buttons[0]);
    fireEvent.click(buttons[1]);
    expect(mockedGetSaleHistoryDetail).toHaveBeenCalledTimes(2);

    await act(async () => first.resolve(detail(1)));
    expect(screen.queryByText("Cajero 1")).toBeNull();
    await act(async () => second.resolve(detail(2)));
    expect(screen.getByText("Cajero 2")).toBeTruthy();
    expect(screen.getByRole("dialog").textContent).toContain("POS-00000002");
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
    fireEvent.click(screen.getAllByRole("button", { name: "Ver detalle" })[0]);
    await act(async () => undefined);

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("No se pudo cargar el detalle de la venta.");
    expect(alert.textContent).not.toContain("SQLITE_CORRUPT");
  });
});
