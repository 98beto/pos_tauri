import type { CreateInventoryMovementInput } from "../../types/inventory_movement";

const STORAGE_KEY_PREFIX = "pos.inventory-movement-intent.user.";

export type PendingManualMovement = {
  version: 1;
  userId: number;
  operationToken: string;
  productId: number;
  reason: "compra" | "ajuste";
  type: "entrada" | "salida";
  quantity: string;
};

export type ClearPendingManualMovementResult = "cleared" | "absent" | "mismatch" | "error";

function storageKey(userId: number) {
  return `${STORAGE_KEY_PREFIX}${userId}`;
}

function parsePendingMovement(raw: string): PendingManualMovement | null {
  try {
    const value = JSON.parse(raw) as Partial<PendingManualMovement>;
    if (
      value.version !== 1 ||
      !Number.isSafeInteger(value.userId) ||
      typeof value.operationToken !== "string" ||
      !Number.isSafeInteger(value.productId) ||
      !["compra", "ajuste"].includes(value.reason ?? "") ||
      !["entrada", "salida"].includes(value.type ?? "") ||
      typeof value.quantity !== "string"
    ) return null;
    return value as PendingManualMovement;
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

export function manualMovementInput(intent: PendingManualMovement): CreateInventoryMovementInput {
  return {
    product_id: intent.productId,
    reason: intent.reason,
    type: intent.type,
    quantity: intent.quantity,
    operation_token: intent.operationToken,
  };
}
