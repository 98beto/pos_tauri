export interface Product {
  id: number;
  name: string;
  price: number;
  stock: number;
  sku: string;
  brand_id: number;
  category_id: number;
  created_at: string;
  updated_at: string;
}

export interface CreateProductInput {
  name: string;
  price: string;
  stock: number;
  sku: string;
  brand_id: number;
  category_id: number;
}

export type UpdateProductInput = Omit<CreateProductInput, "stock">;

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
  inventory_units: number;
  brand_count: number;
  inventory_value: number;
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
  cart_quantity: number;
  available_stock: number;
  brand_id: number;
  brand_name: string;
  category_id: number;
  category_name: string;
}
