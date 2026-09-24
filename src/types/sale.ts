import type { SaleDetail } from "./sale_detail";

export interface Sale {
  id: number;
  user_id: number;
  cashier_name: string;
  cashier_email: string;
  checkout_token: string;
  cart_fingerprint: string;
  sale_date: string;
  total: number;
  payment_method: PaymentMethod;
  cash_received: number | null;
  change_amount: number | null;
  created_at: string;
  updated_at: string;
}

export type PaymentMethod = "efectivo" | "tarjeta" | "transferencia";

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
  item_count: number;
  user_id: number;
  user_name: string;
}

export interface SalesViewStats {
  sale_count: number;
  total_sold: number;
  average_sale: number;
  items_sold: number;
}

export interface SalesView {
  items: SalesViewRow[];
  stats: SalesViewStats;
}

export interface SaleHistoryLine {
  product_id: number;
  product_name: string;
  sku: string;
  quantity: number;
  unit_price: number;
  subtotal: number;
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
  item_count: number;
  total: number;
  cash_received: number | null;
  change_amount: number | null;
}
