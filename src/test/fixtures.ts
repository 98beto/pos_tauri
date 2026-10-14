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
  line_count: 0,
  subtotal: 0,
  discount_type: "none",
  discount_value: 0,
  discount_amount: 0,
  total: 0,
  cart_fingerprint: "empty-cart",
};

export const cart: Cart = {
  lines: [{
    presentation_id: 101,
    presentation_name: "Bolsa 250 g",
    presentation_barcode: null,
    product_id: 11,
    product_name: "Cafe molido",
    sku: "CAFE-1",
    base_unit: "gram",
    mode: "fixed",
    base_quantity: 250,
    catalog_unit_price: 1250,
    applied_unit_price: 1250,
    price_overridden: false,
    entered_quantity: 1,
    base_consumed: 250,
    stock_base: 1_250,
    available: true,
    line_total: 1250,
  }],
  line_count: 1,
  subtotal: 1250,
  discount_type: "none",
  discount_value: 0,
  discount_amount: 0,
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
