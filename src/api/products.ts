import { invoke } from "@tauri-apps/api/core";
import type {
  CreateProductWithPresentationsInput,
  Product,
  ProductCodeMatch,
  ProductSelectorFilters,
  ProductSelectorRow,
  ProductView,
  ProductViewFilters,
  UpdateProductWithPresentationsInput,
} from "../types/product";

export function getProductsView(
  filters: ProductViewFilters = {},
): Promise<ProductView> {
  return invoke<ProductView>("get_products_view", { filters });
}

export function listProductSelector(
  filters: ProductSelectorFilters = {},
): Promise<ProductSelectorRow[]> {
  return invoke<ProductSelectorRow[]>("list_product_selector", { filters });
}

export function listProducts(): Promise<Product[]> {
  return invoke<Product[]>("list_products");
}

export function findProductBySku(sku: string): Promise<Product | null> {
  return invoke<Product | null>("find_product_by_sku", { sku });
}

export function findProductByCode(code: string): Promise<ProductCodeMatch | null> {
  return invoke<ProductCodeMatch | null>("find_product_by_code", { code });
}

export function createProductWithPresentations(
  input: CreateProductWithPresentationsInput,
): Promise<Product> {
  return invoke<Product>("create_product_with_presentations", { input });
}

export function updateProductWithPresentations(
  id: number,
  input: UpdateProductWithPresentationsInput,
): Promise<Product> {
  return invoke<Product>("update_product_with_presentations", { id, input });
}
