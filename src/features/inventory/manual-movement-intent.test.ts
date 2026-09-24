import { describe, expect, it } from "vitest";
import {
  clearPendingManualMovement,
  manualMovementInput,
  readPendingManualMovement,
  savePendingManualMovement,
  type PendingManualMovement,
} from "./manual-movement-intent";

function intent(userId: number, token = `token-${userId}`): PendingManualMovement {
  return {
    version: 1,
    userId,
    operationToken: token,
    productId: 11,
    reason: "ajuste",
    type: "salida",
    quantity: "2",
  };
}

describe("manual movement intent storage", () => {
  it("isolates intents by user and maps the exact backend input", () => {
    savePendingManualMovement(intent(1));
    savePendingManualMovement(intent(2));
    expect(readPendingManualMovement(1)).toEqual(intent(1));
    expect(readPendingManualMovement(2)).toEqual(intent(2));
    expect(manualMovementInput(intent(1))).toEqual({
      product_id: 11,
      reason: "ajuste",
      type: "salida",
      quantity: "2",
      operation_token: "token-1",
    });
  });

  it("only clears the currently stored matching token", () => {
    const pending = intent(7, "current-token");
    savePendingManualMovement(pending);
    expect(clearPendingManualMovement(7, "old-token")).toBe("mismatch");
    expect(readPendingManualMovement(7)).toEqual(pending);
    expect(clearPendingManualMovement(7, "current-token")).toBe("cleared");
    expect(readPendingManualMovement(7)).toBeNull();
    expect(clearPendingManualMovement(7, "current-token")).toBe("absent");
  });

  it("does not report empty or corrupt records as absent or cleared", () => {
    const key = "pos.inventory-movement-intent.user.7";
    localStorage.setItem(key, "");
    expect(clearPendingManualMovement(7, "current-token")).toBe("mismatch");
    expect(localStorage.getItem(key)).toBe("");

    localStorage.setItem(key, "{invalid");
    expect(clearPendingManualMovement(7, "current-token")).toBe("mismatch");
    expect(localStorage.getItem(key)).toBe("{invalid");
  });

  it("ignores corrupt and cross-user records", () => {
    localStorage.setItem("pos.inventory-movement-intent.user.7", "{invalid");
    localStorage.setItem("pos.inventory-movement-intent.user.8", JSON.stringify(intent(9)));
    expect(readPendingManualMovement(7)).toBeNull();
    expect(readPendingManualMovement(8)).toBeNull();
  });
});
