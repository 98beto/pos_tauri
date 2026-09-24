import type { Cart } from "../types/cart";
import type { User } from "../types/user";

export const user: User = {
  id: 7,
  first_name: "Ada",
  last_name: "Lovelace",
  email: "ada@example.com",
  created_at: "2026-01-01",
  updated_at: "2026-01-01",
};

export const emptyCart: Cart = {
  lines: [],
  item_count: 0,
  subtotal: 0,
  total: 0,
  cart_fingerprint: "empty-cart",
};

export const cart: Cart = {
  lines: [{
    product_id: 11,
    sku: "CAFE-1",
    name: "Cafe molido",
    unit_price: 1250,
    quantity: 1,
    stock: 5,
    available: true,
    subtotal: 1250,
  }],
  item_count: 1,
  subtotal: 1250,
  total: 1250,
  cart_fingerprint: "cart-11-1",
};

export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}
