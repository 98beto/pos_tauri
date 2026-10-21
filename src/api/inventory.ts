import { invoke } from "@tauri-apps/api/core";
import type {
  ConfirmPurchasePriceCorrectionInput,
  CreatePurchaseInput,
  CreateStockCountInput,
  InventoryMovement,
  InventoryProductSearchResult,
  InventoryView,
  InventoryViewFilters,
  LookupInventoryMovementInput,
  ProductInventoryDetail,
  ProductInventoryDetailFilters,
  PurchasePriceCorrectionQuote,
  PurchaseQuote,
  QuotePurchaseInput,
  QuotePurchasePriceCorrectionInput,
  QuoteStockCountInput,
  StockCountQuote,
} from "../types/inventory_movement";

export function lookupInventoryMovement(
  input: LookupInventoryMovementInput,
): Promise<InventoryMovement | null> {
  return invoke<InventoryMovement | null>("lookup_inventory_movement", { input });
}

export function quotePurchase(input: QuotePurchaseInput): Promise<PurchaseQuote> {
  return invoke<PurchaseQuote>("quote_purchase", { input });
}

export function createPurchase(
  input: CreatePurchaseInput,
): Promise<InventoryMovement> {
  return invoke<InventoryMovement>("create_purchase", { input });
}

export function quoteStockCount(input: QuoteStockCountInput): Promise<StockCountQuote> {
  return invoke<StockCountQuote>("quote_stock_count", { input });
}

export function createStockCount(
  input: CreateStockCountInput,
): Promise<InventoryMovement> {
  return invoke<InventoryMovement>("create_stock_count", { input });
}

export function searchInventoryProducts(
  search: string | undefined,
  page: number,
): Promise<InventoryProductSearchResult> {
  return invoke<InventoryProductSearchResult>("search_inventory_products", {
    search,
    page,
  });
}

export function getInventoryView(
  filters: InventoryViewFilters = {},
): Promise<InventoryView> {
  return invoke<InventoryView>("get_inventory_view", { filters });
}

export function getProductInventoryDetail(
  productId: number,
  filters: ProductInventoryDetailFilters = {},
): Promise<ProductInventoryDetail> {
  return invoke<ProductInventoryDetail>("get_product_inventory_detail", {
    productId,
    filters,
  });
}

export function listInventoryMovements(): Promise<InventoryMovement[]> {
  return invoke<InventoryMovement[]>("list_inventory_movements");
}

export function quotePurchasePriceCorrection(
  input: QuotePurchasePriceCorrectionInput,
): Promise<PurchasePriceCorrectionQuote> {
  return invoke<PurchasePriceCorrectionQuote>("quote_purchase_price_correction", { input });
}

export function confirmPurchasePriceCorrection(
  input: ConfirmPurchasePriceCorrectionInput,
): Promise<PurchasePriceCorrectionQuote> {
  return invoke<PurchasePriceCorrectionQuote>("confirm_purchase_price_correction", { input });
}
