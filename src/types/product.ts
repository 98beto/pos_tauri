import type {
  CreateProductPresentationInput,
  ProductPresentation,
  UpdateProductPresentationInput,
} from "./product_presentation";

export type BaseUnit = "piece" | "gram" | "milliliter" | "centimeter";

export interface Product {
  id: number;
  name: string;
  price: number;
  stock: number;
  stock_base: number;
  base_unit: BaseUnit;
  sku: string;
  brand_id: number;
  category_id: number;
  created_at: string;
  updated_at: string;
  presentations: ProductPresentation[];
  default_presentation: ProductPresentation;
}

export interface CreateProductWithPresentationsInput {
  name: string;
  sku: string;
  brand_id: number;
  category_id: number;
  base_unit: BaseUnit;
  presentations: CreateProductPresentationInput[];
}

export interface UpdateProductWithPresentationsInput {
  name: string;
  sku: string;
  brand_id: number;
  category_id: number;
  base_unit: BaseUnit;
  presentations: UpdateProductPresentationInput[];
}

export interface ProductCodeMatch {
  product: Product;
  match_kind: "sku" | "barcode";
  presentation_id?: number;
}

export interface ProductViewFilters {
  search?: string;
  brand_id?: number;
  category_id?: number;
}

export interface ProductViewRow extends Product {
  brand_name: string;
  category_name: string;
}

export interface ProductViewStats {
  product_count: number;
  piece_stock: number;
  gram_stock: number;
  milliliter_stock: number;
  centimeter_stock: number;
  brand_count: number;
}

export interface ProductView {
  items: ProductViewRow[];
  stats: ProductViewStats;
}

export interface ProductSelectorFilters {
  search?: string;
  category_id?: number;
}

export interface ProductSelectorRow {
  id: number;
  name: string;
  sku: string;
  price: number;
  stock: number;
  stock_base: number;
  base_unit: BaseUnit;
  cart_quantity: number;
  available_stock: number;
  brand_id: number;
  brand_name: string;
  category_id: number;
  category_name: string;
  presentations: ProductPresentation[];
  default_presentation: ProductPresentation;
}

export interface InventoryProductRow {
  id: number;
  name: string;
  sku: string;
  stock: number;
  base_unit: BaseUnit;
}
