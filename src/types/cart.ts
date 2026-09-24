import type { PaymentMethod } from "./sale";

export interface CartLine {
  product_id: number;
  sku: string;
  name: string;
  unit_price: number;
  quantity: number;
  stock: number;
  available: boolean;
  subtotal: number;
}

export interface Cart {
  lines: CartLine[];
  item_count: number;
  subtotal: number;
  total: number;
  cart_fingerprint: string;
}

export interface CashChangeQuote {
  cash_received: number;
  change: number;
  total: number;
}

export interface CheckoutCartInput {
  checkout_token: string;
  cart_fingerprint: string;
  payment_method: PaymentMethod;
  cash_received?: string;
}
