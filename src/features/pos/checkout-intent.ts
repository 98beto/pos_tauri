import type { CheckoutCartInput } from "../../types/cart";
import type { PaymentMethod } from "../../types/sale";

const LEGACY_STORAGE_KEY = "pos.checkout-intent";
const STORAGE_KEY_PREFIX = "pos.checkout-intent.user.";

export type PendingCheckout = {
  version: 1;
  userId: number;
  checkoutToken: string;
  cartFingerprint: string;
  paymentMethod: PaymentMethod;
  cashReceived?: string;
};

function storageKey(userId: number) {
  return `${STORAGE_KEY_PREFIX}${userId}`;
}

function parsePendingCheckout(raw: string): PendingCheckout | null {
  try {
    const value = JSON.parse(raw) as Partial<PendingCheckout>;
    if (
      value.version !== 1 ||
      !Number.isSafeInteger(value.userId) ||
      typeof value.checkoutToken !== "string" ||
      typeof value.cartFingerprint !== "string" ||
      !["efectivo", "tarjeta", "transferencia"].includes(value.paymentMethod ?? "") ||
      (value.cashReceived !== undefined && typeof value.cashReceived !== "string")
    ) return null;
    return value as PendingCheckout;
  } catch {
    return null;
  }
}

function migrateLegacyPendingCheckout() {
  const raw = localStorage.getItem(LEGACY_STORAGE_KEY);
  if (!raw) return;
  const intent = parsePendingCheckout(raw);
  if (intent) {
    const key = storageKey(intent.userId);
    if (localStorage.getItem(key) === null) localStorage.setItem(key, raw);
  }
  if (localStorage.getItem(LEGACY_STORAGE_KEY) === raw) localStorage.removeItem(LEGACY_STORAGE_KEY);
}

export function readPendingCheckout(userId: number): PendingCheckout | null {
  try {
    migrateLegacyPendingCheckout();
    const raw = localStorage.getItem(storageKey(userId));
    if (!raw) return null;
    const intent = parsePendingCheckout(raw);
    return intent?.userId === userId ? intent : null;
  } catch {
    return null;
  }
}

export function savePendingCheckout(intent: PendingCheckout) {
  localStorage.setItem(storageKey(intent.userId), JSON.stringify(intent));
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
    ...(intent.paymentMethod === "efectivo" ? { cash_received: intent.cashReceived } : {}),
  };
}
