import type { SaleDetail } from "./sale_detail";
import type { BaseUnit } from "./product";
import type { PresentationMode } from "./product_presentation";

export interface Sale {
  id: number;
  user_id: number;
  cashier_name: string;
  cashier_email: string;
  checkout_token: string;
  cart_fingerprint: string;
  sale_date: string;
  subtotal: number;
  discount_type: CartDiscountType;
  discount_value: number;
  discount_amount: number;
  total: number;
  payment_method: PaymentMethod;
  cash_received: number | null;
  change_amount: number | null;
  created_at: string;
  updated_at: string;
}

export type PaymentMethod = "efectivo" | "tarjeta" | "transferencia";
export type CartDiscountType = "none" | "percentage" | "fixed";

export interface SaleReceipt {
  sale: Sale;
  details: SaleDetail[];
}

export interface SalesViewFilters {
  search?: string;
  payment_method?: PaymentMethod;
}

export interface SalesViewRow {
  id: number;
  ticket_number: string;
  sale_date: string;
  payment_method: PaymentMethod;
  total: number;
  line_count: number;
  user_id: number;
  user_name: string;
}

export interface SalesViewStats {
  sale_count: number;
  total_sold: number;
  average_sale: number;
  lines_sold: number;
}

export interface SalesView {
  items: SalesViewRow[];
  stats: SalesViewStats;
}

export interface SaleHistoryLine {
  product_id: number;
  presentation_id: number | null;
  product_name: string;
  sku: string;
  presentation_name: string | null;
  presentation_barcode: string | null;
  base_unit: BaseUnit | null;
  presentation_mode: PresentationMode | null;
  presentation_base_quantity: number | null;
  quantity: number;
  catalog_unit_price: number | null;
  unit_price: number;
  entered_quantity: number;
  base_consumed: number;
  subtotal: number | null;
}

export interface SaleHistoryDetail {
  id: number;
  ticket_number: string;
  payment_method: PaymentMethod;
  sale_date: string;
  user_id: number;
  user_name: string;
  user_email: string;
  lines: SaleHistoryLine[];
  line_count: number;
  subtotal: number;
  discount_type: CartDiscountType;
  discount_value: number;
  discount_amount: number;
  line_totals_verifiable: boolean;
  total: number;
  cash_received: number | null;
  change_amount: number | null;
}
