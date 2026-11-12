import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { cashIncome, cashWithdraw, closeCashSession, getCashSessionDetail, getCashState, listCashSessions, openCashSession } from "../../api/cash";
import { cashIncomeEvent, cashSaleEvent, openCash } from "../../test/cash-fixtures";
import { deferred, user } from "../../test/fixtures";
import type { CashSession, CashSessionDetail } from "../../types/cash";
import { readCashIntent, saveCashIntent } from "./cash-intent";
import { CashView } from "./CashView";
import { formatDateTime } from "../../lib/format";

vi.mock("../../api/cash", () => ({ getCashState: vi.fn(), listCashSessions: vi.fn(), getCashSessionDetail: vi.fn(), openCashSession: vi.fn(), cashIncome: vi.fn(), cashWithdraw: vi.fn(), closeCashSession: vi.fn() }));
const detail: CashSessionDetail = { session: openCash, operations: [], timeline: [] };
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(getCashState).mockResolvedValue(openCash);
  vi.mocked(listCashSessions).mockResolvedValue([openCash]);
  vi.mocked(getCashSessionDetail).mockResolvedValue(detail);
  vi.mocked(openCashSession).mockResolvedValue(detail);
  vi.mocked(cashIncome).mockResolvedValue(detail);
  vi.mocked(cashWithdraw).mockResolvedValue(detail);
  vi.mocked(closeCashSession).mockResolvedValue({ session: { ...openCash, status: "closed", counted_amount: 999, difference: -1 }, operations: [], timeline: [] });
});
async function loaded() {
  const onNotice = vi.fn(); const onSessionRequired = vi.fn();
  const view = render(<CashView user={user} onNotice={onNotice} onSessionRequired={onSessionRequired} />);
  await act(async () => undefined);
  return { ...view, onNotice, onSessionRequired };
}
const click = async (name: string) => { const buttons = screen.getAllByRole("button", { name }); fireEvent.click(buttons[buttons.length - 1]); await act(async () => undefined); };
const amount = (label: string, value: string) => fireEvent.change(screen.getByLabelText(label, { exact: false }), { target: { value } });

describe("CashView", () => {
  it("shows net cash sales alongside distinct manual movements and non-income opening/closing amounts", async () => {
    const base = { ...cashIncomeEvent, session_id: 2 };
    vi.mocked(getCashSessionDetail).mockResolvedValue({ ...detail, timeline: [
      { ...base, event_key: "operation:1", operation_id: 1, kind: "open", amount: 0, reason: "" },
      cashSaleEvent,
      { ...base, amount: 15000, reason: "Fondo adicional" },
      { ...base, event_key: "operation:5", operation_id: 5, kind: "withdraw", amount: 500, reason: "Pago menor" },
      { ...base, event_key: "operation:6", operation_id: 6, kind: "close", amount: 0, reason: "" },
    ] });
    await loaded(); await click("Ver caja #2");
    const dialog = within(screen.getByRole("dialog"));
    expect(dialog.getByRole("heading", { name: "Movimientos de caja" })).toBeTruthy();
    const rows = dialog.getAllByRole("listitem");
    expect(rows).toHaveLength(5);
    expect(rows[0].textContent).toContain("Fondo inicial · $0.00");
    const sale = within(rows[1]);
    expect(sale.getByText("Venta en efectivo · +$150.00")).toBeTruthy();
    expect(sale.getByText("POS-00000001")).toBeTruthy();
    expect(sale.queryByRole("link")).toBeNull();
    expect(sale.getByText("Recibido: $200.00")).toBeTruthy();
    expect(sale.getByText("Cambio: $50.00")).toBeTruthy();
    expect(sale.getByText(`Ada <ada@example.com> · ${formatDateTime(cashSaleEvent.created_at)}`)).toBeTruthy();
    expect(rows[2].textContent).toContain("Ingreso manual · +$150.00");
    expect(rows[2].textContent).toContain("Fondo adicional");
    expect(rows[3].textContent).toContain("Retiro · −$5.00");
    expect(rows[3].textContent).toContain("Pago menor");
    expect(rows[4].textContent).toContain("Cierre · Efectivo contado · $0.00");
  });
  it.each(["income", "withdraw", "close"] as const)("pins a %s form to its opening session and discards old input when focus finds another session", async (kind) => {
    await loaded(); await click(kind === "income" ? "Registrar ingreso" : kind === "withdraw" ? "Registrar retiro" : "Cerrar caja");
    const label = kind === "close" ? "Efectivo contado" : "Importe";
    amount(label, "7.29");
    if (kind !== "close") amount("Motivo", "Datos para A");
    expect(screen.getByText("Caja destino #2")).toBeTruthy();
    if (kind === "close") { await click("Revisar cierre final"); expect(within(screen.getByRole("dialog")).getByText("Caja destino #2")).toBeTruthy(); }
    vi.mocked(getCashState).mockResolvedValue({ ...openCash, id: 3 });
    fireEvent.focus(window); await act(async () => undefined);
    expect(screen.queryByLabelText(new RegExp(label))).toBeNull();
    expect(screen.queryByRole("dialog", { name: "Confirmar cierre final" })).toBeNull();
    expect(screen.getByRole("alert").textContent).toContain("Abre un formulario nuevo");
    expect(cashIncome).not.toHaveBeenCalled(); expect(cashWithdraw).not.toHaveBeenCalled(); expect(closeCashSession).not.toHaveBeenCalled();
    await click(kind === "income" ? "Registrar ingreso" : kind === "withdraw" ? "Registrar retiro" : "Cerrar caja");
    expect(screen.getByText("Caja destino #3")).toBeTruthy();
    expect((screen.getByLabelText(new RegExp(label)) as HTMLInputElement).value).toBe("");
    if (kind !== "close") expect((screen.getByLabelText("Motivo", { exact: false }) as HTMLInputElement).value).toBe("");
  });

  it.each(["income", "withdraw", "close"] as const)("invalidates a %s form when its original session is now closed", async (kind) => {
    await loaded(); await click(kind === "income" ? "Registrar ingreso" : kind === "withdraw" ? "Registrar retiro" : "Cerrar caja");
    amount(kind === "close" ? "Efectivo contado" : "Importe", "7.29");
    vi.mocked(getCashState).mockResolvedValue({ ...openCash, status: "closed" });
    fireEvent.focus(window); await act(async () => undefined);
    expect(screen.queryByText("Caja destino #2")).toBeNull();
    expect(screen.queryByLabelText(/pesos/)).toBeNull();
    expect(cashIncome).not.toHaveBeenCalled(); expect(cashWithdraw).not.toHaveBeenCalled(); expect(closeCashSession).not.toHaveBeenCalled();
  });

  it.each(["income", "withdraw", "close"] as const)("invalidates a %s form when getCashState reports no open session", async (kind) => {
    await loaded(); await click(kind === "income" ? "Registrar ingreso" : kind === "withdraw" ? "Registrar retiro" : "Cerrar caja");
    amount(kind === "close" ? "Efectivo contado" : "Importe", "7.29");
    vi.mocked(getCashState).mockResolvedValue(null);
    fireEvent.focus(window); await act(async () => undefined);
    expect(screen.queryByLabelText(/pesos/)).toBeNull();
    expect(screen.getByRole("alert").textContent).toContain("cambio o se cerro");
    expect(cashIncome).not.toHaveBeenCalled(); expect(cashWithdraw).not.toHaveBeenCalled(); expect(closeCashSession).not.toHaveBeenCalled();
  });
  it.each(["income", "withdraw", "close"] as const)("retries a lost %s response with the durable original inputs", async (kind) => {
    const api = vi.mocked(kind === "income" ? cashIncome : kind === "withdraw" ? cashWithdraw : closeCashSession);
    api.mockRejectedValueOnce(new Error("response lost")).mockResolvedValueOnce(detail);
    const pending = { version: 1 as const, userId: user.id, token: `lost-${kind}`, kind, sessionId: 1, amount: kind === "close" ? 0 : 29, reason: kind === "close" ? "" : "Fondo" };
    saveCashIntent(pending);
    await loaded(); await click("Reintentar operacion");
    expect(readCashIntent(user.id)).toEqual(pending);
    await click("Reintentar operacion");
    expect(api).toHaveBeenCalledTimes(2);
    expect(api.mock.calls[0][0]).toEqual(api.mock.calls[1][0]);
    expect(api.mock.calls[1][0]).toMatchObject({ session_id: 1, operation_token: `lost-${kind}` });
    expect(readCashIntent(user.id)).toBeUndefined();
  });

  it("keeps a confirmed intent locked if storage cleanup fails; retry only recovers the same token", async () => {
    const pending = { version: 1 as const, userId: user.id, token: "confirmed", kind: "income" as const, sessionId: 2, amount: 29, reason: "Fondo" };
    saveCashIntent(pending);
    await loaded();
    const spy = vi.spyOn(Storage.prototype, "removeItem").mockImplementation(() => { throw new Error("storage failed"); });
    await click("Reintentar operacion");
    expect(screen.getByRole("dialog", { name: "Detalle de caja" })).toBeTruthy();
    expect(readCashIntent(user.id)).toEqual(pending);
    spy.mockRestore();
    await click("Cerrar detalle"); await click("Reintentar operacion");
    expect(vi.mocked(cashIncome).mock.calls[0][0]).toEqual(vi.mocked(cashIncome).mock.calls[1][0]);
    expect(readCashIntent(user.id)).toBeUndefined();
  });

  it("blocks writes after a local read failure and routes backend authentication errors", async () => {
    const spy = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("storage unavailable"); });
    vi.mocked(getCashState).mockRejectedValue({ category: "authentication", code: "SESSION_REQUIRED", message: "Sesion requerida" });
    const { onSessionRequired } = await loaded();
    expect(onSessionRequired).toHaveBeenCalledOnce();
    expect(screen.getAllByRole("alert").some((alert) => alert.textContent?.includes("No se pudo leer"))).toBe(true);
    expect(screen.queryByRole("button", { name: "Abrir caja" })).toBeNull();
    expect(openCashSession).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it("opens with exact cents and persists before dispatch; lost responses retry the same token after remount", async () => {
    vi.mocked(getCashState).mockResolvedValue(null);
    vi.mocked(listCashSessions).mockResolvedValue([]);
    vi.mocked(openCashSession).mockImplementationOnce(async (input) => {
      expect(readCashIntent(user.id)?.token).toBe(input.operation_token);
      throw new Error("lost response");
    });
    const first = await loaded();
    await click("Abrir caja");
    amount("Fondo inicial", "0.29");
    fireEvent.submit(screen.getByLabelText(/Fondo inicial/).closest("form")!);
    await act(async () => undefined);
    const original = vi.mocked(openCashSession).mock.calls[0][0];
    expect(original.opening_amount).toBe(29);
    expect((screen.getByLabelText(/Fondo inicial/) as HTMLInputElement).disabled).toBe(true);
    first.unmount();
    await loaded();
    await click("Reintentar operacion");
    expect(vi.mocked(openCashSession).mock.calls[1][0]).toEqual(original);
    expect(readCashIntent(user.id)).toBeUndefined();
    expect(screen.getByRole("dialog", { name: "Detalle de caja" })).toBeTruthy();
  });

  it("deduplicates double submits and shows the confirmed detail even if reloading fails", async () => {
    const response = deferred<CashSessionDetail>();
    vi.mocked(cashIncome).mockReturnValueOnce(response.promise);
    await loaded(); await click("Registrar ingreso");
    amount("Importe", "1.01"); amount("Motivo", "  Cambio  ");
    const form = screen.getByLabelText(/Importe/).closest("form")!;
    fireEvent.submit(form); fireEvent.submit(form);
    expect(cashIncome).toHaveBeenCalledOnce();
    expect(cashIncome).toHaveBeenCalledWith(expect.objectContaining({ amount: 101, reason: "Cambio", session_id: 2 }));
    vi.mocked(getCashState).mockRejectedValueOnce(new Error("reload"));
    const updated = { ...detail, session: { ...openCash, expected_amount: 1101, breakdown: { ...openCash.breakdown, income: 101 } } };
    await act(async () => response.resolve(updated));
    expect(readCashIntent(user.id)).toBeUndefined();
    expect(screen.queryByText("Operacion pendiente: Registrar ingreso")).toBeNull();
    expect(within(screen.getByRole("dialog")).getByText("$11.01")).toBeTruthy();
    await click("Cerrar detalle");
    expect(screen.queryByRole("button", { name: "Registrar ingreso" })).toBeNull();
    expect(cashIncome).toHaveBeenCalledOnce();
  });

  it("validates amount and reason, and renders backend withdrawal errors without inventing a balance", async () => {
    await loaded(); await click("Registrar retiro");
    amount("Importe", "0"); amount("Motivo", "Renta");
    await click("Registrar retiro");
    expect(screen.getByRole("alert").textContent).toContain("mayor que cero");
    amount("Importe", "0.001"); await click("Registrar retiro");
    expect(screen.getByRole("alert").textContent).toContain("dos decimales");
    amount("Importe", "20"); amount("Motivo", "   "); await click("Registrar retiro");
    expect(screen.getByRole("alert").textContent).toContain("motivo");
    expect(cashWithdraw).not.toHaveBeenCalled();
    amount("Motivo", "Renta");
    vi.mocked(cashWithdraw).mockRejectedValueOnce({ category: "conflict", code: "INSUFFICIENT_CASH_BALANCE", message: "El retiro supera el esperado actual." });
    await click("Registrar retiro");
    expect(screen.getByRole("alert").textContent).toContain("supera el esperado");
    expect(readCashIntent(user.id)).toBeUndefined();
  });

  it("confirms the exact expected, counted and difference and cannot dismiss while closing", async () => {
    const response = deferred<CashSessionDetail>();
    vi.mocked(closeCashSession).mockReturnValue(response.promise);
    await loaded(); await click("Cerrar caja"); amount("Efectivo contado", "9.99");
    await click("Revisar cierre final");
    const dialog = screen.getByRole("dialog", { name: "Confirmar cierre final" });
    expect(within(dialog).getByText("$10.00")).toBeTruthy();
    expect(within(dialog).getByText("$9.99")).toBeTruthy();
    expect(within(dialog).getByText("-$0.01")).toBeTruthy();
    await click("Confirmar cierre");
    expect((within(dialog).getByRole("button", { name: "Volver" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "Confirmar cierre final" })).toBeTruthy();
    expect(closeCashSession).toHaveBeenCalledWith(expect.objectContaining({ session_id: 2, counted_amount: 999 }));
    vi.mocked(getCashState).mockResolvedValue(null);
    await act(async () => response.resolve({ session: { ...openCash, status: "closed", counted_amount: 999, difference: -1 }, operations: [], timeline: [] }));
    expect(within(screen.getByRole("dialog", { name: "Detalle de caja" })).getByText("Caja #2 · Cerrada")).toBeTruthy();
  });

  it("shows historical null reconciliation, min/max dates and immutable audit detail", async () => {
    const historical: CashSession = { ...openCash, id: 1, historical: true, status: "closed", opening_amount: null, expected_amount: null, counted_amount: null, difference: null, opened_by: null, opening_operator: null, opened_at: "2020-01-01 10:00:00", closed_at: "2021-12-31 15:00:00", breakdown: { ...openCash.breakdown, cash_sales: 300 } };
    vi.mocked(getCashState).mockResolvedValue(null);
    vi.mocked(listCashSessions).mockResolvedValue([historical]);
    vi.mocked(getCashSessionDetail).mockResolvedValue({ session: historical, operations: [], timeline: [{ ...cashSaleEvent, session_id: 1, amount: 300, cash_received: null, change_amount: null }] });
    await loaded(); await click("Ver caja #1");
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("Caja #1 · Historica · Cerrada")).toBeTruthy();
    expect(within(dialog).getAllByText("No registrado")).toHaveLength(4);
    expect(within(dialog).getByText(/Primera venta:/).textContent).toContain("2020");
    expect(within(dialog).getByText(/Ultima venta:/).textContent).toContain("2021");
    expect(within(dialog).queryByRole("button", { name: "Cerrar caja" })).toBeNull();
    expect(within(dialog).getAllByText("$0.00", { selector: "dd" }).length).toBeGreaterThan(0); // Known breakdown zero, not fabricated reconciliation.
    expect(within(dialog).getByText("Venta en efectivo · +$3.00")).toBeTruthy();
    expect(within(dialog).queryByText(/^Recibido:/)).toBeNull();
    expect(within(dialog).queryByText(/^Cambio:/)).toBeNull();
    expect(within(dialog).queryByText(/^Fondo inicial ·/)).toBeNull();
    expect(within(dialog).queryByText(/^Cierre · Efectivo contado ·/)).toBeNull();
  });

  it("never dispatches when storage cannot persist the intention", async () => {
    await loaded(); await click("Registrar ingreso"); amount("Importe", "1"); amount("Motivo", "Cambio");
    const spy = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("full"); });
    await click("Registrar ingreso");
    expect(cashIncome).not.toHaveBeenCalled();
    expect(screen.getByRole("alert").textContent).toContain("No se envio");
    spy.mockRestore();
  });

  it("recovers a persisted movement with its original session, never the currently open one", async () => {
    saveCashIntent({ version: 1, userId: user.id, token: "original", kind: "income", sessionId: 1, amount: 10, reason: "Cambio" });
    vi.mocked(cashIncome).mockResolvedValue({ session: { ...openCash, id: 1, status: "closed" }, operations: [{ id: 4, session_id: 1, kind: "income", amount: 10, reason: "Cambio", user_id: user.id, operator: "Otro operador <otro@example.com>", operation_token: "original", created_at: "2026-01-01 12:00:00" }], timeline: [cashIncomeEvent] });
    await loaded(); await click("Reintentar operacion");
    expect(cashIncome).toHaveBeenCalledWith({ operation_token: "original", session_id: 1, amount: 10, reason: "Cambio" });
    expect(within(screen.getByRole("dialog")).getByText(/Otro operador/)).toBeTruthy();
  });

  it("ignores stale state responses on focus and after unmount", async () => {
    const first = deferred<CashSession | null>();
    vi.mocked(getCashState).mockReturnValueOnce(first.promise);
    const view = await loaded();
    fireEvent.focus(window); await act(async () => undefined);
    expect(screen.getByRole("button", { name: "Cerrar caja" })).toBeTruthy();
    await act(async () => first.resolve(null));
    expect(screen.getByRole("button", { name: "Cerrar caja" })).toBeTruthy();
    view.unmount();
  });
});
