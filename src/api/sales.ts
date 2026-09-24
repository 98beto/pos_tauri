import { invoke } from "@tauri-apps/api/core";
import type {
  Sale,
  SaleHistoryDetail,
  SalesView,
  SalesViewFilters,
} from "../types/sale";

export function getSalesView(filters: SalesViewFilters = {}): Promise<SalesView> {
  return invoke<SalesView>("get_sales_view", { filters });
}

export function getSaleHistoryDetail(
  saleId: number,
): Promise<SaleHistoryDetail> {
  return invoke<SaleHistoryDetail>("get_sale_history_detail", { saleId });
}

export function listSales(): Promise<Sale[]> {
  return invoke<Sale[]>("list_sales");
}
