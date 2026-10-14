import { describe, expect, it } from "vitest";
import {
  clearPendingManualMovement,
  manualMovementInput,
  readPendingManualMovement,
  replacePendingManualMovement,
  savePendingManualMovement,
  type PendingManualMovement,
  type PendingManualMovementV2,
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
    quoteFingerprint: "inventory-quote-v1:test",
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

  it("round-trips v2 intents and maps every optional backend field", () => {
    const pending = intentV2(3);
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
      operation_token: "token-v2-3",
      quote_fingerprint: "inventory-quote-v1:test",
    });
  });

  it("replaces a legacy entry only when the stored record still matches", () => {
    const legacy = { ...intent(3, "legacy-entry"), reason: "compra" as const, type: "entrada" as const };
    const replacement = intentV2(3, "legacy-entry");
    savePendingManualMovement(legacy);
    expect(replacePendingManualMovement(legacy, replacement)).toBe(true);
    expect(readPendingManualMovement(3)).toEqual(replacement);
    expect(replacePendingManualMovement(legacy, intentV2(3, "other"))).toBe(false);
    expect(readPendingManualMovement(3)).toEqual(replacement);
  });

  it("keeps legacy v1 payloads free of v2 fields", () => {
    expect(manualMovementInput(intent(4))).toEqual({
      product_id: 11,
      reason: "ajuste",
      type: "salida",
      quantity: "2",
      operation_token: "token-4",
    });
  });

  it("reopens pre-fingerprint v2 metadata as an unquoted draft", () => {
    const previous = intentV2(5);
    const { quoteFingerprint: _quoteFingerprint, ...stored } = previous;
    localStorage.setItem("pos.inventory-movement-intent.user.5", JSON.stringify(stored));
    const reopened = readPendingManualMovement(5);
    expect(reopened).toEqual({ ...stored, quoteFingerprint: null });
    expect(manualMovementInput(reopened!)).toEqual(expect.objectContaining({
      operation_token: previous.operationToken,
      supplier: previous.supplier,
      document_reference: previous.documentReference,
      note: previous.note,
      quote_fingerprint: null,
    }));
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
