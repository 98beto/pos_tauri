import { invoke } from "@tauri-apps/api/core";
import type {
  CreateProductInput,
  Product,
  ProductSelectorFilters,
  ProductSelectorRow,
  ProductView,
  ProductViewFilters,
  UpdateProductInput,
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

export function createProduct(input: CreateProductInput): Promise<Product> {
  return invoke<Product>("create_product", { input });
}

export function updateProduct(
  id: number,
  input: UpdateProductInput,
): Promise<Product> {
  return invoke<Product>("update_product", { id, input });
}
