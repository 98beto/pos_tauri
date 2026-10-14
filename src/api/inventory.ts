import { invoke } from "@tauri-apps/api/core";
import type {
  CreateInventoryMovementInput,
  InventoryMovement,
  InventoryMovementQuote,
  InventoryProductSearchResult,
  InventoryView,
  InventoryViewFilters,
  ProductInventoryDetail,
  ProductInventoryDetailFilters,
  QuoteInventoryMovementInput,
} from "../types/inventory_movement";

export function createInventoryMovement(
  input: CreateInventoryMovementInput,
): Promise<InventoryMovement> {
  return invoke<InventoryMovement>("create_inventory_movement", { input });
}

export function quoteInventoryMovement(
  input: QuoteInventoryMovementInput,
): Promise<InventoryMovementQuote> {
  return invoke<InventoryMovementQuote>("quote_inventory_movement", { input });
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
