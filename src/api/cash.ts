import { invoke } from "@tauri-apps/api/core";
import type {
  CashSession,
  CashSessionDetail,
  OpenCashInput,
  CashMovementInput,
  CloseCashInput,
} from "../types/cash";

export function getCashState(): Promise<CashSession | null> {
  return invoke("get_cash_state");
}
export function listCashSessions(): Promise<CashSession[]> {
  return invoke("list_cash_sessions");
}
export function getCashSessionDetail(sessionId: number): Promise<CashSessionDetail> {
  return invoke("get_cash_session_detail", { sessionId });
}
export function openCashSession(input: OpenCashInput): Promise<CashSessionDetail> {
  return invoke("open_cash_session", { input });
}
export function cashIncome(input: CashMovementInput): Promise<CashSessionDetail> {
  return invoke("cash_income", { input });
}
export function cashWithdraw(input: CashMovementInput): Promise<CashSessionDetail> {
  return invoke("cash_withdraw", { input });
}
export function closeCashSession(input: CloseCashInput): Promise<CashSessionDetail> {
  return invoke("close_cash_session", { input });
}
