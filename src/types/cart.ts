import type { PaymentMethod } from "./sale";
import type { BaseUnit } from "./product";
import type { PresentationMode } from "./product_presentation";

export interface CartLine {
  presentation_id: number;
  presentation_name: string;
  presentation_barcode: string | null;
  product_id: number;
  product_name: string;
  sku: string;
  base_unit: BaseUnit;
  mode: PresentationMode;
  base_quantity: number;
  catalog_unit_price: number;
  applied_unit_price: number;
  price_overridden: boolean;
  entered_quantity: number;
  base_consumed: number;
  stock_base: number;
  available: boolean;
  line_total: number;
}

export interface Cart {
  lines: CartLine[];
  line_count: number;
  subtotal: number;
  discount_type: CartDiscountType;
  discount_value: number;
  discount_amount: number;
  total: number;
  cart_fingerprint: string;
}

export type CartDiscountType = "none" | "percentage" | "fixed";

export type CartDiscountQuote = Pick<
  Cart,
  "subtotal" | "discount_type" | "discount_value" | "discount_amount" | "total"
>;

export interface CashChangeQuote {
  cash_received: number;
  change: number;
  total: number;
}

export interface CheckoutCartInput {
  checkout_token: string;
  cart_fingerprint: string;
  payment_method: PaymentMethod;
  cash_received?: string | null;
}
