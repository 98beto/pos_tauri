import { describe, expect, it, vi } from "vitest";
import { clearCashIntent, formatCashAmount, parseCashAmount, readCashIntent, saveCashIntent, type CashIntent } from "./cash-intent";

const intent: CashIntent = { version: 1, userId: 1, token: "durable", kind: "withdraw", sessionId: 2, amount: 29, reason: "Cambio" };
describe("cash intents and exact cents", () => {
  it.each([["0", 0], ["0.29", 29], ["1.01", 101], ["00012.3", 1230], ["90071992547409.91", Number.MAX_SAFE_INTEGER]])("parses %s exactly", (text, expected) => {
    expect(parseCashAmount(text)).toBe(expected);
  });
  it.each(["-1", "+1", "1.001", "1e2", "1,000", "", "Infinity", "90071992547409.92", "0.0001"])("rejects %s without rounding", (text) => {
    expect(() => parseCashAmount(text)).toThrow();
  });
  it("renders exact signed differences even at the safe limit", () => {
    expect(formatCashAmount(Number.MAX_SAFE_INTEGER)).toBe("$90,071,992,547,409.91");
    expect(formatCashAmount(-Number.MAX_SAFE_INTEGER)).toBe("-$90,071,992,547,409.91");
    expect(formatCashAmount(-1)).toBe("-$0.01");
  });
  it("persists all inputs per user, refuses competing tokens and clears only its token", () => {
    saveCashIntent(intent);
    expect(readCashIntent(1)).toEqual(intent);
    expect(readCashIntent(2)).toBeUndefined();
    expect(() => saveCashIntent({ ...intent, token: "new" })).toThrow();
    clearCashIntent({ ...intent, token: "old" });
    expect(readCashIntent(1)).toEqual(intent);
    clearCashIntent(intent);
    expect(readCashIntent(1)).toBeUndefined();
  });
  it("does not treat inaccessible storage as an empty queue", () => {
    const spy = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("denied"); });
    expect(() => readCashIntent(1)).toThrow();
    expect(() => saveCashIntent(intent)).toThrow();
    spy.mockRestore();
  });
});
