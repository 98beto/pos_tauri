export type PresentationMode = "fixed" | "measured";

export interface ProductPresentation {
  id: number;
  product_id: number;
  name: string;
  mode: PresentationMode;
  base_quantity: number;
  price: number;
  barcode: string | null;
  is_default: boolean;
  is_active: boolean;
  is_conversion_locked?: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateProductPresentationInput {
  name: string;
  mode: PresentationMode;
  base_quantity: number;
  price: string;
  barcode?: string | null;
  is_default: boolean;
  is_active?: boolean;
}

export interface UpdateProductPresentationInput {
  id?: number | null;
  name: string;
  mode: PresentationMode;
  base_quantity: number;
  price: string;
  barcode?: string | null;
  is_default: boolean;
  is_active: boolean;
}
