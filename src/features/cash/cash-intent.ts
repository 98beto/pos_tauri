import { cashIncome, cashWithdraw, closeCashSession, openCashSession } from "../../api/cash";
import type { CashSessionDetail } from "../../types/cash";

export type CashAction = "open" | "income" | "withdraw" | "close";
export type CashIntent = {
  version: 1; userId: number; token: string; kind: CashAction;
  sessionId?: number; amount: number; reason: string;
};
const key = (userId: number) => `pos.cash-intent.user.${userId}`;

/** Parse decimal pesos without any floating point multiplication or rounding. */
export function parseCashAmount(text: string): number {
  const match = /^(\d+)(?:\.(\d{1,2}))?$/.exec(text.trim());
  if (!match) throw new Error("Ingresa pesos con hasta dos decimales, sin signos ni separadores.");
  const cents = Number(`${match[1]}${(match[2] ?? "").padEnd(2, "0")}`);
  if (!Number.isSafeInteger(cents)) throw new Error("El importe excede el limite seguro.");
  return cents;
}

/** Keep the last cent visible even at MAX_SAFE_INTEGER (including differences). */
export function formatCashAmount(cents: number): string {
  const digits = String(Math.abs(cents)).padStart(3, "0");
  return `${cents < 0 ? "-" : ""}$${digits.slice(0, -2).replace(/\B(?=(\d{3})+(?!\d))/g, ",")}.${digits.slice(-2)}`;
}

// Read failures are intentional: do not overwrite a possibly unresolved operation.
export function readCashIntent(userId: number): CashIntent | undefined {
  const raw = localStorage.getItem(key(userId));
  if (!raw) return;
  const value = JSON.parse(raw) as CashIntent;
  if (value.version !== 1 || value.userId !== userId || !value.token ||
      !["open", "income", "withdraw", "close"].includes(value.kind) ||
      !Number.isSafeInteger(value.amount) || value.amount < 0 || typeof value.reason !== "string" ||
      (value.kind !== "open" && (!Number.isSafeInteger(value.sessionId) || value.sessionId! <= 0))) {
    throw new Error("La intencion de caja guardada no es valida. No se enviaran operaciones nuevas.");
  }
  return value;
}
export function saveCashIntent(intent: CashIntent) {
  const existing = readCashIntent(intent.userId);
  if (existing && existing.token !== intent.token) throw new Error("Existe una operacion de caja pendiente.");
  localStorage.setItem(key(intent.userId), JSON.stringify(intent));
}
export function clearCashIntent(intent: CashIntent) {
  if (readCashIntent(intent.userId)?.token === intent.token) localStorage.removeItem(key(intent.userId));
}
const requests = new Map<string, Promise<CashSessionDetail>>();
export function submitCashIntent(intent: CashIntent): Promise<CashSessionDetail> {
  const requestKey = `${intent.userId}:${intent.token}`;
  const existing = requests.get(requestKey);
  if (existing) return existing;
  const movement = { operation_token: intent.token, session_id: intent.sessionId!, amount: intent.amount, reason: intent.reason };
  const request = intent.kind === "open" ? openCashSession({ operation_token: intent.token, opening_amount: intent.amount })
    : intent.kind === "close" ? closeCashSession({ operation_token: intent.token, session_id: intent.sessionId!, counted_amount: intent.amount })
    : intent.kind === "income" ? cashIncome(movement) : cashWithdraw(movement);
  requests.set(requestKey, request);
  void request.then(() => requests.delete(requestKey), () => requests.delete(requestKey));
  return request;
}
