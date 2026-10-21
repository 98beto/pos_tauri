import type {
  CreatePurchaseInput,
  CreateStockCountInput,
  LookupInventoryMovementInput,
} from "../../types/inventory_movement";

const STORAGE_KEY_PREFIX = "pos.inventory-movement-intent.user.";

export type PendingManualMovementV1 = {
  version: 1;
  userId: number;
  operationToken: string;
  productId: number;
  reason: "compra" | "ajuste";
  type: "entrada" | "salida";
  quantity: string;
};

export type PendingManualMovementV2 = {
  version: 2;
  userId: number;
  operationToken: string;
  productId: number;
  presentationId: number | null;
  reason: "compra" | "ajuste";
  type: "entrada" | "salida";
  quantity: string;
  unitPrice: string | null;
  supplier: string | null;
  documentReference: string | null;
  note: string | null;
  quoteFingerprint: string | null;
};

export type PendingManualMovementV3 = {
  version: 3;
  userId: number;
  operationToken: string;
  productId: number;
  presentationId: number | null;
  reason: "compra" | "ajuste";
  type: "entrada" | "salida";
  quantity: string;
  supplier: string | null;
  documentReference: string | null;
  note: string | null;
  quoteFingerprint: string;
};

export type PendingManualMovementV4 = {
  version: 4;
  userId: number;
  operationToken: string;
  productId: number;
  presentationId: number | null;
  reason: "compra" | "ajuste";
  type: "entrada" | "salida";
  quantity: string;
  unitPrice: string | null;
  supplier: string | null;
  documentReference: string | null;
  note: string | null;
  quoteFingerprint: string;
};

type PendingManualMovementV5Base = {
  version: 5;
  userId: number;
  operationToken: string;
  productId: number;
  quoteFingerprint: string;
};

export type PendingPurchaseV5 = PendingManualMovementV5Base & {
  kind: "purchase";
  presentationId: number | null;
  quantity: string;
  purchaseUnitPrice: string;
  supplier: string;
  documentReference: string;
  note: string | null;
};

export type PendingStockCountV5 = PendingManualMovementV5Base & {
  kind: "stock_count";
  countedStock: string;
  adjustmentReason: string;
};

export type PendingManualMovementV5 = PendingPurchaseV5 | PendingStockCountV5;
export type LegacyPendingManualMovement =
  | PendingManualMovementV1
  | PendingManualMovementV2
  | PendingManualMovementV3
  | PendingManualMovementV4;
export type PendingManualMovement = LegacyPendingManualMovement | PendingManualMovementV5;

export type ClearPendingManualMovementResult = "cleared" | "absent" | "mismatch" | "error";

function storageKey(userId: number) {
  return `${STORAGE_KEY_PREFIX}${userId}`;
}

function optionalString(value: unknown) {
  return value === null || typeof value === "string";
}

function hasOnlyKeys(value: Record<string, unknown>, keys: readonly string[]) {
  const allowed = new Set(keys);
  return Object.keys(value).every((key) => allowed.has(key));
}

function parsePendingMovement(raw: string): PendingManualMovement | null {
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (
      !Number.isSafeInteger(value.userId) ||
      typeof value.operationToken !== "string" ||
      !Number.isSafeInteger(value.productId)
    ) return null;

    if (
      value.version === 1 &&
      typeof value.quantity === "string" &&
      ["compra", "ajuste"].includes(String(value.reason)) &&
      ["entrada", "salida"].includes(String(value.type))
    ) return value as unknown as PendingManualMovementV1;
    if (
      value.version === 2 &&
      typeof value.quantity === "string" &&
      (value.presentationId === null || Number.isSafeInteger(value.presentationId)) &&
      ["compra", "ajuste"].includes(String(value.reason)) &&
      ["entrada", "salida"].includes(String(value.type)) &&
      optionalString(value.unitPrice) &&
      optionalString(value.supplier) &&
      optionalString(value.documentReference) &&
      optionalString(value.note) &&
      (value.quoteFingerprint === undefined || optionalString(value.quoteFingerprint))
    ) return { ...value, quoteFingerprint: value.quoteFingerprint ?? null } as unknown as PendingManualMovementV2;
    if (
      value.version === 3 &&
      typeof value.quantity === "string" &&
      (value.presentationId === null || Number.isSafeInteger(value.presentationId)) &&
      ["compra", "ajuste"].includes(String(value.reason)) &&
      ["entrada", "salida"].includes(String(value.type)) &&
      optionalString(value.supplier) &&
      optionalString(value.documentReference) &&
      optionalString(value.note) &&
      typeof value.quoteFingerprint === "string" && value.quoteFingerprint.length > 0
    ) return value as unknown as PendingManualMovementV3;
    if (
      value.version === 4 &&
      typeof value.quantity === "string" &&
      (value.presentationId === null || Number.isSafeInteger(value.presentationId)) &&
      ["compra", "ajuste"].includes(String(value.reason)) &&
      ["entrada", "salida"].includes(String(value.type)) &&
      optionalString(value.unitPrice) &&
      optionalString(value.supplier) &&
      optionalString(value.documentReference) &&
      optionalString(value.note) &&
      typeof value.quoteFingerprint === "string" && value.quoteFingerprint.length > 0
    ) return value as unknown as PendingManualMovementV4;

    const commonV5Keys = ["version", "kind", "userId", "operationToken", "productId", "quoteFingerprint"];
    if (
      value.version === 5 &&
      value.kind === "purchase" &&
      hasOnlyKeys(value, [...commonV5Keys, "presentationId", "quantity", "purchaseUnitPrice", "supplier", "documentReference", "note"]) &&
      (value.presentationId === null || Number.isSafeInteger(value.presentationId)) &&
      typeof value.quantity === "string" &&
      typeof value.purchaseUnitPrice === "string" &&
      typeof value.supplier === "string" &&
      typeof value.documentReference === "string" &&
      optionalString(value.note) &&
      typeof value.quoteFingerprint === "string" && value.quoteFingerprint.length > 0
    ) return value as unknown as PendingPurchaseV5;
    if (
      value.version === 5 &&
      value.kind === "stock_count" &&
      hasOnlyKeys(value, [...commonV5Keys, "countedStock", "adjustmentReason"]) &&
      typeof value.countedStock === "string" &&
      typeof value.adjustmentReason === "string" &&
      typeof value.quoteFingerprint === "string" && value.quoteFingerprint.length > 0
    ) return value as unknown as PendingStockCountV5;
    return null;
  } catch {
    return null;
  }
}

export function readPendingManualMovement(userId: number): PendingManualMovement | null {
  try {
    const raw = localStorage.getItem(storageKey(userId));
    if (!raw) return null;
    const intent = parsePendingMovement(raw);
    return intent?.userId === userId ? intent : null;
  } catch {
    return null;
  }
}

export function savePendingManualMovement(intent: PendingManualMovement) {
  const stored = readPendingManualMovement(intent.userId);
  if (stored && stored.operationToken !== intent.operationToken) {
    throw new Error("Resuelve la operacion pendiente antes de iniciar otra.");
  }
  localStorage.setItem(storageKey(intent.userId), JSON.stringify(intent));
}

export function replacePendingManualMovement(
  current: PendingManualMovement,
  replacement: PendingManualMovementV5,
): boolean {
  try {
    const key = storageKey(current.userId);
    const raw = localStorage.getItem(key);
    if (raw === null) return false;
    const stored = parsePendingMovement(raw);
    if (
      stored?.userId !== current.userId ||
      stored.operationToken !== current.operationToken ||
      replacement.userId !== current.userId ||
      replacement.operationToken !== current.operationToken ||
      localStorage.getItem(key) !== raw
    ) return false;
    localStorage.setItem(key, JSON.stringify(replacement));
    return localStorage.getItem(key) === JSON.stringify(replacement);
  } catch {
    return false;
  }
}

export function clearPendingManualMovement(userId: number, token: string): ClearPendingManualMovementResult {
  try {
    const key = storageKey(userId);
    const raw = localStorage.getItem(key);
    if (raw === null) return "absent";
    const intent = parsePendingMovement(raw);
    if (intent?.userId !== userId || intent.operationToken !== token || localStorage.getItem(key) !== raw) {
      return "mismatch";
    }
    localStorage.removeItem(key);
    return localStorage.getItem(key) === null ? "cleared" : "mismatch";
  } catch {
    return "error";
  }
}

export function manualMovementInput(
  intent: PendingManualMovement,
): CreatePurchaseInput | CreateStockCountInput {
  if (intent.version !== 5) {
    throw new Error("Legacy inventory intents must be quoted again before confirmation");
  }
  if (intent.kind === "purchase") {
    return {
      product_id: intent.productId,
      presentation_id: intent.presentationId,
      quantity: intent.quantity,
      purchase_unit_price: intent.purchaseUnitPrice,
      supplier: intent.supplier,
      document_reference: intent.documentReference,
      note: intent.note,
      operation_token: intent.operationToken,
      quote_fingerprint: intent.quoteFingerprint,
    };
  }
  return {
    product_id: intent.productId,
    counted_stock: intent.countedStock,
    adjustment_reason: intent.adjustmentReason,
    operation_token: intent.operationToken,
    quote_fingerprint: intent.quoteFingerprint,
  };
}

export function manualMovementLookupInput(
  intent: PendingManualMovement,
): LookupInventoryMovementInput {
  return { operation_token: intent.operationToken };
}
