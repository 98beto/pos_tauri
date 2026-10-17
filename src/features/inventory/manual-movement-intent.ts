import type { CreateInventoryMovementInput } from "../../types/inventory_movement";

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
  reason: "compra" | "ajuste" | "valoracion";
  type: "entrada" | "salida" | "valoracion";
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
  reason: "compra" | "ajuste" | "valoracion";
  type: "entrada" | "salida" | "valoracion";
  quantity: string;
  unitPrice: string | null;
  supplier: string | null;
  documentReference: string | null;
  note: string | null;
  quoteFingerprint: string;
};

export type PendingManualMovement = PendingManualMovementV1 | PendingManualMovementV2 | PendingManualMovementV3 | PendingManualMovementV4;

export type ClearPendingManualMovementResult = "cleared" | "absent" | "mismatch" | "error";

function storageKey(userId: number) {
  return `${STORAGE_KEY_PREFIX}${userId}`;
}

function optionalString(value: unknown) {
  return value === null || typeof value === "string";
}

function parsePendingMovement(raw: string): PendingManualMovement | null {
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    const commonIsValid =
      !Number.isSafeInteger(value.userId) ||
      typeof value.operationToken !== "string" ||
      !Number.isSafeInteger(value.productId) ||
      typeof value.quantity !== "string";
    if (commonIsValid) return null;
    if (
      value.version === 1 &&
      ["compra", "ajuste"].includes(String(value.reason)) &&
      ["entrada", "salida"].includes(String(value.type))
    ) return value as unknown as PendingManualMovementV1;
    if (
      value.version === 2 &&
      (value.presentationId === null || Number.isSafeInteger(value.presentationId)) &&
      ["compra", "ajuste", "valoracion"].includes(String(value.reason)) &&
      ["entrada", "salida", "valoracion"].includes(String(value.type)) &&
      optionalString(value.unitPrice) &&
      optionalString(value.supplier) &&
      optionalString(value.documentReference) &&
      optionalString(value.note) &&
      (value.quoteFingerprint === undefined || optionalString(value.quoteFingerprint))
    ) return { ...value, quoteFingerprint: value.quoteFingerprint ?? null } as unknown as PendingManualMovementV2;
    if (
      value.version === 3 &&
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
      (value.presentationId === null || Number.isSafeInteger(value.presentationId)) &&
      ["compra", "ajuste", "valoracion"].includes(String(value.reason)) &&
      ["entrada", "salida", "valoracion"].includes(String(value.type)) &&
      optionalString(value.unitPrice) &&
      optionalString(value.supplier) &&
      optionalString(value.documentReference) &&
      optionalString(value.note) &&
      typeof value.quoteFingerprint === "string" && value.quoteFingerprint.length > 0
    ) return value as unknown as PendingManualMovementV4;
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
  localStorage.setItem(storageKey(intent.userId), JSON.stringify(intent));
}

export function replacePendingManualMovement(
  current: PendingManualMovement,
  replacement: PendingManualMovementV4,
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

export function manualMovementInput(intent: PendingManualMovementV4): CreateInventoryMovementInput {
  const input: CreateInventoryMovementInput = {
    product_id: intent.productId,
    reason: intent.reason,
    type: intent.type,
    quantity: intent.quantity,
    operation_token: intent.operationToken,
  };
  input.presentation_id = intent.presentationId;
  input.unit_price = intent.unitPrice;
  input.supplier = intent.supplier;
  input.document_reference = intent.documentReference;
  input.note = intent.note;
  input.quote_fingerprint = intent.quoteFingerprint;
  return input;
}
