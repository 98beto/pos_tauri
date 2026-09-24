import { describe, expect, it } from "vitest";
import { checkoutInput, clearPendingCheckout, readPendingCheckout, savePendingCheckout, type PendingCheckout } from "./checkout-intent";

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
  it("isolates pending checkout data by user", () => {
    savePendingCheckout(intent(1));
    savePendingCheckout(intent(2));
    expect(readPendingCheckout(1)).toEqual(intent(1));
    expect(readPendingCheckout(2)).toEqual(intent(2));
  });

  it("only clears the matching token and maps the backend payload", () => {
    const pending = intent(7, "current-token");
    savePendingCheckout(pending);
    clearPendingCheckout(7, "old-token");
    expect(readPendingCheckout(7)).toEqual(pending);
    expect(checkoutInput(pending)).toEqual({
      checkout_token: "current-token",
      cart_fingerprint: "cart-7",
      payment_method: "efectivo",
      cash_received: "20.00",
    });
    clearPendingCheckout(7, "current-token");
    expect(readPendingCheckout(7)).toBeNull();
  });

  it("ignores corrupt or cross-user records", () => {
    localStorage.setItem("pos.checkout-intent.user.7", "{invalid");
    localStorage.setItem("pos.checkout-intent.user.8", JSON.stringify(intent(9)));
    expect(readPendingCheckout(7)).toBeNull();
    expect(readPendingCheckout(8)).toBeNull();
  });
});
