import { describe, expect, it, vi } from "vitest";
import { archiveBlockedCheckout, CheckoutStorageError, checkoutInput, clearPendingCheckout, readPendingCheckout, savePendingCheckout, type PendingCheckout } from "./checkout-intent";

function intent(userId: number, token = `token-${userId}`): PendingCheckout {
  return {
    version: 1,
    userId,
    checkoutToken: token,
    cartFingerprint: `cart-${userId}`,
    paymentMethod: "efectivo",
    cashReceived: "20.00",
  };
}

describe("checkout intent storage", () => {
  it("persists and sends the original cash session without adding one to legacy intents", () => {
    const pending = { ...intent(7), expectedCashSessionId: 22 };
    savePendingCheckout(pending);
    expect(readPendingCheckout(7)?.expectedCashSessionId).toBe(22);
    expect(checkoutInput(readPendingCheckout(7)!)).toMatchObject({ expected_cash_session_id: 22 });
    expect(checkoutInput(intent(1))).not.toHaveProperty("expected_cash_session_id");
  });
  it("isolates pending checkout data by user", () => {
    savePendingCheckout(intent(1));
    savePendingCheckout(intent(2));
    expect(readPendingCheckout(1)).toEqual({ ...intent(1), version: 2 });
    expect(readPendingCheckout(2)).toEqual({ ...intent(2), version: 2 });
  });

  it("only clears the matching token and maps the backend payload", () => {
    const pending = intent(7, "current-token");
    savePendingCheckout(pending);
    clearPendingCheckout(7, "old-token");
    expect(readPendingCheckout(7)).toEqual({ ...pending, version: 2 });
    expect(checkoutInput(pending)).toEqual({
      checkout_token: "current-token",
      cart_fingerprint: "cart-7",
      payment_method: "efectivo",
      cash_received: "20.00",
    });
    clearPendingCheckout(7, "current-token");
    expect(readPendingCheckout(7)).toBeNull();
  });

  it("blocks corrupt or cross-user records without changing their bytes", () => {
    localStorage.setItem("pos.checkout-intent.user.7", "{invalid");
    localStorage.setItem("pos.checkout-intent.user.8", JSON.stringify(intent(9)));
    expect(() => readPendingCheckout(7)).toThrow(CheckoutStorageError);
    expect(() => readPendingCheckout(8)).toThrow(CheckoutStorageError);
    expect(() => savePendingCheckout(intent(7))).toThrow();
    expect(localStorage.getItem("pos.checkout-intent.user.7")).toBe("{invalid");
  });

  it("migrates a valid legacy intent once without overwriting a current user intent", () => {
    const legacy = intent(7, "legacy-token");
    localStorage.setItem("pos.checkout-intent", JSON.stringify(legacy));
    expect(readPendingCheckout(7)).toEqual(legacy);
    expect(localStorage.getItem("pos.checkout-intent")).toBeNull();

    const current = intent(7, "current-token");
    clearPendingCheckout(7, legacy.checkoutToken);
    savePendingCheckout(current);
    localStorage.setItem("pos.checkout-intent", JSON.stringify(legacy));
    expect(() => readPendingCheckout(7)).toThrow();
    expect(localStorage.getItem("pos.checkout-intent")).toBe(JSON.stringify(legacy));
    expect(JSON.parse(localStorage.getItem("pos.checkout-intent.user.7")!).checkoutToken).toBe("current-token");
  });

  it("refuses another token or changed inputs and does not overwrite after a transient read failure", () => {
    savePendingCheckout(intent(7));
    const raw = localStorage.getItem("pos.checkout-intent.user.7");
    expect(() => savePendingCheckout(intent(7, "other-token"))).toThrow();
    expect(() => savePendingCheckout({ ...intent(7), cashReceived: "30.00" })).toThrow();
    const spy = vi.spyOn(Storage.prototype, "getItem").mockImplementationOnce(() => { throw new Error("denied"); });
    expect(() => savePendingCheckout(intent(7, "new-token"))).toThrow();
    spy.mockRestore();
    expect(localStorage.getItem("pos.checkout-intent.user.7")).toBe(raw);
    expect(readPendingCheckout(7)?.checkoutToken).toBe("token-7");
  });

  it("preserves uninterpretable legacy data and requires a durable archive before explicit resolution", () => {
    const raw = "{legacy,unknown}";
    localStorage.setItem("pos.checkout-intent", raw);
    let blocked!: CheckoutStorageError;
    try { readPendingCheckout(7); } catch (value) { blocked = value as CheckoutStorageError; }
    expect(blocked.records).toEqual([{ key: "pos.checkout-intent", raw }]);
    const spy = vi.spyOn(Storage.prototype, "setItem").mockImplementationOnce(() => { throw new Error("full"); });
    expect(() => archiveBlockedCheckout(7, blocked)).toThrow();
    spy.mockRestore();
    expect(localStorage.getItem("pos.checkout-intent")).toBe(raw);
    archiveBlockedCheckout(7, blocked);
    expect(readPendingCheckout(7)).toBeNull();
    const key = Object.keys(localStorage).find((key) => key.startsWith("pos.checkout-intent.reviewed.user.7."))!;
    expect(JSON.parse(localStorage.getItem(key)!)).toEqual([{ key: "pos.checkout-intent", raw }]);
  });
});
