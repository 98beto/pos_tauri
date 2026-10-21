import { describe, expect, it } from "vitest";
import {
  clearPendingManualMovement,
  manualMovementLookupInput,
  manualMovementInput,
  readPendingManualMovement,
  replacePendingManualMovement,
  savePendingManualMovement,
  type PendingManualMovementV2,
  type PendingPurchaseV5,
  type PendingStockCountV5,
} from "./manual-movement-intent";

function legacyIntent(userId: number, token = `legacy-${userId}`): PendingManualMovementV2 {
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

function purchaseIntent(userId: number, token = `purchase-${userId}`): PendingPurchaseV5 {
  return {
    version: 5,
    kind: "purchase",
    userId,
    operationToken: token,
    productId: 11,
    presentationId: 101,
    quantity: "3",
    purchaseUnitPrice: "12.50",
    supplier: "Proveedor",
    documentReference: "FAC-9",
    note: "Entrega completa",
    quoteFingerprint: "purchase-quote-v3:new",
  };
}

function stockCountIntent(userId: number, token = `count-${userId}`): PendingStockCountV5 {
  return {
    version: 5,
    kind: "stock_count",
    userId,
    operationToken: token,
    productId: 11,
    countedStock: "275",
    adjustmentReason: "Conteo mensual",
    quoteFingerprint: "stock-count-quote-v3:new",
  };
}

describe("manual movement intent storage", () => {
  it("round-trips a purchase and maps the exact backend input", () => {
    const pending = purchaseIntent(3);
    savePendingManualMovement(pending);
    expect(readPendingManualMovement(3)).toEqual(pending);
    expect(manualMovementInput(pending)).toEqual({
      product_id: 11,
      presentation_id: 101,
      quantity: "3",
      purchase_unit_price: "12.50",
      supplier: "Proveedor",
      document_reference: "FAC-9",
      note: "Entrega completa",
      operation_token: "purchase-3",
      quote_fingerprint: "purchase-quote-v3:new",
    });
  });

  it("round-trips a stock count without commercial fields", () => {
    const pending = stockCountIntent(4);
    savePendingManualMovement(pending);
    expect(readPendingManualMovement(4)).toEqual(pending);
    expect(manualMovementInput(pending)).toEqual({
      product_id: 11,
      counted_stock: "275",
      adjustment_reason: "Conteo mensual",
      operation_token: "count-4",
      quote_fingerprint: "stock-count-quote-v3:new",
    });
  });

  it("loads legacy context but never converts it to a confirmable input", () => {
    const legacy = legacyIntent(5, "same-token");
    savePendingManualMovement(legacy);
    expect(readPendingManualMovement(5)).toEqual(legacy);
    expect(() => manualMovementInput(legacy)).toThrow(/quoted again/);
    expect(manualMovementLookupInput(legacy)).toEqual({ operation_token: "same-token" });

    const replacement = purchaseIntent(5, "same-token");
    expect(replacePendingManualMovement(legacy, replacement)).toBe(true);
    expect(readPendingManualMovement(5)).toEqual(replacement);
  });

  it("does not recover retired valuation intents", () => {
    localStorage.setItem(
      "pos.inventory-movement-intent.user.13",
      JSON.stringify({ ...legacyIntent(13), reason: "valoracion", type: "valoracion" }),
    );
    expect(readPendingManualMovement(13)).toBeNull();
  });

  it("uses only the token to check a lost response and supports an absent lookup", async () => {
    const lookup = async ({ operation_token }: { operation_token: string }) =>
      operation_token === "applied" ? { id: 41 } : null;

    await expect(lookup(manualMovementLookupInput(legacyIntent(12, "applied"))))
      .resolves.toEqual({ id: 41 });
    await expect(lookup(manualMovementLookupInput(legacyIntent(12, "absent"))))
      .resolves.toBeNull();
  });

  it("rejects cross-kind fields in persisted v5 records", () => {
    localStorage.setItem(
      "pos.inventory-movement-intent.user.6",
      JSON.stringify({ ...stockCountIntent(6), supplier: "No permitido" }),
    );
    expect(readPendingManualMovement(6)).toBeNull();
  });

  it("only clears the matching user and token", () => {
    savePendingManualMovement(purchaseIntent(7, "current"));
    expect(clearPendingManualMovement(7, "old")).toBe("mismatch");
    expect(clearPendingManualMovement(7, "current")).toBe("cleared");
    expect(clearPendingManualMovement(7, "current")).toBe("absent");
  });

  it("does not overwrite a concurrently changed local record", () => {
    const legacy = legacyIntent(8, "token");
    savePendingManualMovement(legacy);
    localStorage.setItem("pos.inventory-movement-intent.user.8", JSON.stringify(stockCountIntent(8, "other-token")));
    expect(replacePendingManualMovement(legacy, purchaseIntent(8, "token"))).toBe(false);
    expect(readPendingManualMovement(8)?.operationToken).toBe("other-token");
  });

  it("ignores corrupt and cross-user records", () => {
    localStorage.setItem("pos.inventory-movement-intent.user.9", "{invalid");
    localStorage.setItem("pos.inventory-movement-intent.user.10", JSON.stringify(purchaseIntent(11)));
    expect(readPendingManualMovement(9)).toBeNull();
    expect(readPendingManualMovement(10)).toBeNull();
  });

  it("rejects a different token until the original is resolved", () => {
    const original = purchaseIntent(14);
    savePendingManualMovement(original);
    expect(() => savePendingManualMovement(stockCountIntent(14))).toThrow(/pendiente/);
    expect(readPendingManualMovement(14)).toEqual(original);
    expect(clearPendingManualMovement(14, original.operationToken)).toBe("cleared");
    savePendingManualMovement(stockCountIntent(14));
    expect(readPendingManualMovement(14)).toEqual(stockCountIntent(14));
  });
});
