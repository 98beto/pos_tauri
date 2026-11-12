import type { CheckoutCartInput } from "../../types/cart";
import type { PaymentMethod } from "../../types/sale";

const LEGACY_STORAGE_KEY = "pos.checkout-intent";
const STORAGE_KEY_PREFIX = "pos.checkout-intent.user.";

export type PendingCheckout = {
  version: 1 | 2;
  userId: number;
  checkoutToken: string;
  cartFingerprint: string;
  paymentMethod: PaymentMethod;
  cashReceived?: string;
  expectedCashSessionId?: number;
};

function storageKey(userId: number) {
  return `${STORAGE_KEY_PREFIX}${userId}`;
}

type StoredRecord = { key: string; raw: string };
export class CheckoutStorageError extends Error {
  constructor(public readonly records: StoredRecord[] = []) {
    super(records.length ? "Los datos del cobro guardado requieren revision. Se conservaron sin cambios." : "No se pudo leer el cobro guardado. No se permiten cobros nuevos hasta revisar el almacenamiento.");
  }
}

function parsePendingCheckout(raw: string): PendingCheckout | null {
  try {
    const value = JSON.parse(raw) as Partial<PendingCheckout>;
    if (
      ![1, 2].includes(value.version ?? 0) ||
      !Number.isSafeInteger(value.userId) || value.userId! <= 0 ||
      typeof value.checkoutToken !== "string" || !value.checkoutToken.trim() ||
      typeof value.cartFingerprint !== "string" ||
      !["efectivo", "tarjeta", "transferencia"].includes(value.paymentMethod ?? "") ||
      (value.cashReceived !== undefined && typeof value.cashReceived !== "string") ||
      (value.expectedCashSessionId !== undefined && (!Number.isSafeInteger(value.expectedCashSessionId) || value.expectedCashSessionId <= 0))
    ) return null;
    return value as PendingCheckout;
  } catch {
    return null;
  }
}

function migrateLegacyPendingCheckout() {
  const raw = localStorage.getItem(LEGACY_STORAGE_KEY);
  if (raw === null) return;
  const intent = parsePendingCheckout(raw);
  if (!intent) throw new CheckoutStorageError([{ key: LEGACY_STORAGE_KEY, raw }]);
  const key = storageKey(intent.userId);
  const current = localStorage.getItem(key);
  if (current !== null) {
    const saved = parsePendingCheckout(current);
    if (!saved || saved.userId !== intent.userId || JSON.stringify(checkoutInput(saved)) !== JSON.stringify(checkoutInput(intent))) {
      throw new CheckoutStorageError([{ key: LEGACY_STORAGE_KEY, raw }, { key, raw: current }]);
    }
  } else {
    localStorage.setItem(key, raw);
  }
  if (localStorage.getItem(LEGACY_STORAGE_KEY) === raw) localStorage.removeItem(LEGACY_STORAGE_KEY);
}

export function readPendingCheckout(userId: number): PendingCheckout | null {
  try {
    migrateLegacyPendingCheckout();
    const raw = localStorage.getItem(storageKey(userId));
    if (raw === null) return null;
    const intent = parsePendingCheckout(raw);
    if (!intent || intent.userId !== userId) throw new CheckoutStorageError([{ key: storageKey(userId), raw }]);
    return intent;
  } catch (value) {
    throw value instanceof CheckoutStorageError ? value : new CheckoutStorageError();
  }
}

export function savePendingCheckout(intent: PendingCheckout) {
  const existing = readPendingCheckout(intent.userId);
  if (existing && JSON.stringify(checkoutInput(existing)) !== JSON.stringify(checkoutInput(intent))) {
    throw new CheckoutStorageError([{ key: storageKey(intent.userId), raw: localStorage.getItem(storageKey(intent.userId))! }]);
  }
  localStorage.setItem(storageKey(intent.userId), JSON.stringify({ ...intent, version: 2 }));
}

/** Explicit operator resolution only. Preserve the exact bytes durably before
 * removing blocked records; never turn unreadable storage into an empty queue. */
export function archiveBlockedCheckout(userId: number, error: CheckoutStorageError) {
  if (!error.records.length) throw new CheckoutStorageError();
  for (const record of error.records) {
    if (localStorage.getItem(record.key) !== record.raw) throw new CheckoutStorageError();
  }
  localStorage.setItem(`pos.checkout-intent.reviewed.user.${userId}.${crypto.randomUUID()}`, JSON.stringify(error.records));
  for (const record of error.records) {
    if (localStorage.getItem(record.key) !== record.raw) throw new CheckoutStorageError();
    localStorage.removeItem(record.key);
  }
}

export function clearPendingCheckout(userId: number, token: string) {
  try {
    const key = storageKey(userId);
    const raw = localStorage.getItem(key);
    if (!raw) return;
    const intent = parsePendingCheckout(raw);
    if (intent?.userId === userId && intent.checkoutToken === token && localStorage.getItem(key) === raw) localStorage.removeItem(key);
  } catch { /* Conserva la intencion si no se puede comparar con seguridad. */ }
}

export function checkoutInput(intent: PendingCheckout): CheckoutCartInput {
  return {
    checkout_token: intent.checkoutToken,
    cart_fingerprint: intent.cartFingerprint,
    payment_method: intent.paymentMethod,
    ...(intent.expectedCashSessionId !== undefined ? { expected_cash_session_id: intent.expectedCashSessionId } : {}),
    ...(intent.paymentMethod === "efectivo" ? { cash_received: intent.cashReceived } : {}),
  };
}
