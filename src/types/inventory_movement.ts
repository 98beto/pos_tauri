import type {
  BaseUnit,
  InventoryCostStatus,
  InventoryProductRow,
} from "./product";
import type { PresentationMode } from "./product_presentation";

export type InventoryMovementReason =
  | "compra"
  | "venta"
  | "ajuste"
  | "valoracion";
export type InventoryMovementType = "entrada" | "salida" | "valoracion";

export interface InventoryMovement {
  id: number;
  product_id: number;
  product_name: string;
  sku: string;
  product_name_snapshot: string;
  sku_snapshot: string;
  sale_id: number | null;
  ticket_reference: string | null;
  record_version: number;
  presentation_id: number | null;
  presentation_name: string | null;
  presentation_barcode: string | null;
  base_unit: BaseUnit | null;
  presentation_mode: PresentationMode | null;
  presentation_base_quantity: number | null;
  unit_price_cents: number | null;
  reason: InventoryMovementReason;
  type: InventoryMovementType;
  quantity: number;
  entered_quantity: number | null;
  base_quantity: number | null;
  stock_before: number | null;
  stock_after: number | null;
  movement_cost_cents: number | null;
  inventory_cost_before_cents: number | null;
  inventory_cost_after_cents: number | null;
  supplier: string | null;
  document_reference: string | null;
  note: string | null;
  operation_token: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateInventoryMovementInput {
  product_id: number;
  presentation_id?: number | null;
  reason: InventoryMovementReason;
  type: InventoryMovementType;
  quantity: string;
  unit_price?: string | null;
  supplier?: string | null;
  document_reference?: string | null;
  note?: string | null;
  operation_token: string;
  quote_fingerprint?: string | null;
}

export type QuoteInventoryMovementInput = Omit<
  CreateInventoryMovementInput,
  "supplier" | "document_reference" | "note" | "operation_token"
>;

export interface InventoryMovementQuote {
  product_id: number;
  product_name: string;
  product_sku: string;
  reason: InventoryMovementReason;
  type: InventoryMovementType;
  presentation_id: number | null;
  presentation_name: string | null;
  capture_name: string;
  presentation_mode: PresentationMode | null;
  presentation_base_quantity: number;
  entered_quantity: number;
  entered_quantity_description: string;
  base_quantity: number;
  base_quantity_description: string;
  unit_price_cents: number | null;
  unit_price_description: string | null;
  total_cost_cents: number | null;
  base_unit: BaseUnit;
  quote_fingerprint: string;
}

export interface InventoryProductSearchResult {
  items: InventoryMovementProduct[];
  page: number;
  total: number;
  page_size: number;
}

export interface InventoryMovementProduct extends InventoryProductRow {
  presentations: InventoryMovementPresentation[];
}

export interface InventoryMovementPresentation {
  id: number;
  product_id: number;
  name: string;
  mode: PresentationMode;
  base_quantity: number;
  is_active: boolean;
}

export interface InventoryViewFilters {
  product_search?: string;
  product_page?: number;
  base_unit?: BaseUnit;
  cost_status?: InventoryCostStatus;
  movement_search?: string;
  movement_page?: number;
  reason?: InventoryMovementReason;
  type?: InventoryMovementType;
}

export interface ProductInventoryDetailFilters {
  movement_search?: string;
  reason?: InventoryMovementReason;
  type?: InventoryMovementType;
  date_from?: string;
  date_to?: string;
  page?: number;
}

export interface InventoryViewStats {
  product_count: number;
  piece_stock: number;
  gram_stock: number;
  milliliter_stock: number;
  inventory_value_cents: number;
  products_without_cost: number;
}

export interface InventoryView {
  items: InventoryMovement[];
  products: InventoryMovementProduct[];
  movements: InventoryMovement[];
  product_page: number;
  product_total: number;
  movement_page: number;
  movement_total: number;
  page_size: number;
  stats: InventoryViewStats;
}

export interface ProductInventoryDetail {
  product: InventoryProductRow;
  last_movement_at: string | null;
  movements: InventoryMovement[];
  page: number;
  total: number;
  page_size: number;
}
