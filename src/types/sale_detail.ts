import type { BaseUnit } from "./product";
import type { PresentationMode } from "./product_presentation";

export interface SaleDetail {
  id: number;
  product_id: number;
  sale_id: number;
  record_version: number;
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
  entered_quantity: number | null;
  base_quantity: number | null;
  line_total: number | null;
  created_at: string;
  updated_at: string;
}
