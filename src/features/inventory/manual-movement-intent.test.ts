import { describe, expect, it } from "vitest";
import {
  clearPendingManualMovement,
  manualMovementInput,
  readPendingManualMovement,
  replacePendingManualMovement,
  savePendingManualMovement,
  type PendingManualMovementV2,
  type PendingManualMovementV3,
  type PendingManualMovementV4,
} from "./manual-movement-intent";

function intentV2(userId: number, token = `token-v2-${userId}`): PendingManualMovementV2 {
  return {
    version: 2,
    userId,
    operationToken: token,
    productId: 11,
    presentationId: 101,
    reason: "compra",
    type: "entrada",
    quantity: "3",
    unitPrice: "12.50",
    supplier: "Proveedor",
    documentReference: "FAC-9",
    note: "Entrega completa",
    quoteFingerprint: "inventory-quote-v2:old",
  };
}

function intentV3(userId: number, token = `token-v3-${userId}`): PendingManualMovementV3 {
  return {
    version: 3,
    userId,
    operationToken: token,
    productId: 11,
    presentationId: 101,
    reason: "compra",
    type: "entrada",
    quantity: "3",
    supplier: "Proveedor",
    documentReference: "FAC-9",
    note: "Entrega completa",
    quoteFingerprint: "inventory-quote-v3:new",
  };
}

function intentV4(userId: number, token = `token-v4-${userId}`): PendingManualMovementV4 {
  return {
    version: 4,
    userId,
    operationToken: token,
    productId: 11,
    presentationId: 101,
    reason: "compra",
    type: "entrada",
    quantity: "3",
    unitPrice: "12.50",
    supplier: "Proveedor",
    documentReference: "FAC-9",
    note: "Entrega completa",
    quoteFingerprint: "inventory-quote-v4:new",
  };
}

describe("manual movement intent storage", () => {
  it("round-trips v4 including purchase price and maps the backend input", () => {
    const pending = intentV4(3);
    savePendingManualMovement(pending);
    expect(readPendingManualMovement(3)).toEqual(pending);
    expect(manualMovementInput(pending)).toEqual({
      product_id: 11,
      presentation_id: 101,
      reason: "compra",
      type: "entrada",
      quantity: "3",
      unit_price: "12.50",
      supplier: "Proveedor",
      document_reference: "FAC-9",
      note: "Entrega completa",
      operation_token: "token-v4-3",
      quote_fingerprint: "inventory-quote-v4:new",
    });
  });

  it("loads v2 context and replaces it atomically with reviewed v4", () => {
    const legacy = intentV2(4, "same-token");
    const replacement = intentV4(4, "same-token");
    savePendingManualMovement(legacy);
    expect(readPendingManualMovement(4)).toEqual(legacy);
    expect(replacePendingManualMovement(legacy, replacement)).toBe(true);
    expect(readPendingManualMovement(4)).toEqual(replacement);
    expect(readPendingManualMovement(4)).toEqual(replacement);
  });

  it("loads discarded no-cost v3 only as recovery context", () => {
    const legacy = intentV3(6);
    savePendingManualMovement(legacy);
    expect(readPendingManualMovement(6)).toEqual(legacy);
  });

  it("loads v1 as recoverable context but manualMovementInput only accepts v3", () => {
    const legacy = { version: 1 as const, userId: 5, operationToken: "v1", productId: 12, reason: "ajuste" as const, type: "salida" as const, quantity: "2" };
    savePendingManualMovement(legacy);
    expect(readPendingManualMovement(5)).toEqual(legacy);
  });

  it("only clears the matching user and token", () => {
    savePendingManualMovement(intentV4(7, "current"));
    expect(clearPendingManualMovement(7, "old")).toBe("mismatch");
    expect(clearPendingManualMovement(7, "current")).toBe("cleared");
    expect(clearPendingManualMovement(7, "current")).toBe("absent");
  });

  it("does not overwrite a concurrently changed local record", () => {
    const legacy = intentV2(8, "token");
    savePendingManualMovement(legacy);
    savePendingManualMovement(intentV4(8, "other-token"));
    expect(replacePendingManualMovement(legacy, intentV4(8, "token"))).toBe(false);
    expect(readPendingManualMovement(8)?.operationToken).toBe("other-token");
  });

  it("ignores corrupt and cross-user records", () => {
    localStorage.setItem("pos.inventory-movement-intent.user.9", "{invalid");
    localStorage.setItem("pos.inventory-movement-intent.user.10", JSON.stringify(intentV4(11)));
    expect(readPendingManualMovement(9)).toBeNull();
    expect(readPendingManualMovement(10)).toBeNull();
  });
});
