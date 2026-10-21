import type { BaseUnit, InventoryProductRow } from "./product";
import type { PresentationMode } from "./product_presentation";

export type InventoryMovementReason = "compra" | "venta" | "ajuste";
export type InventoryMovementType = "entrada" | "salida";

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
  origin: "manual" | "sale" | "initial_stock";
  presentation_id: number | null;
  presentation_name: string | null;
  presentation_barcode: string | null;
  base_unit: BaseUnit | null;
  presentation_mode: PresentationMode | null;
  presentation_base_quantity: number | null;
  purchase_unit_price_cents: number | null;
  purchase_total_cents: number | null;
  reason: InventoryMovementReason;
  type: InventoryMovementType;
  quantity: number;
  entered_quantity: number | null;
  base_quantity: number | null;
  stock_before: number | null;
  stock_after: number | null;
  supplier: string | null;
  document_reference: string | null;
  note: string | null;
  operation_token: string | null;
  created_at: string;
  updated_at: string;
}

export interface LookupInventoryMovementInput {
  operation_token: string;
}

export interface QuotePurchaseInput {
  product_id: number;
  presentation_id?: number | null;
  quantity: string;
  purchase_unit_price: string;
  supplier: string;
  document_reference: string;
  note?: string | null;
}

export interface CreatePurchaseInput extends QuotePurchaseInput {
  operation_token: string;
  quote_fingerprint: string;
}

export interface QuoteStockCountInput {
  product_id: number;
  counted_stock: string;
  adjustment_reason: string;
}

export interface CreateStockCountInput extends QuoteStockCountInput {
  operation_token: string;
  quote_fingerprint: string;
}

export interface PurchaseQuote {
  product_id: number;
  product_name: string;
  product_sku: string;
  presentation_id: number | null;
  presentation_name: string | null;
  capture_name: string;
  presentation_mode: PresentationMode | null;
  presentation_base_quantity: number;
  entered_quantity: number;
  base_quantity: number;
  stock_before: number;
  stock_after: number;
  purchase_unit_price_cents: number;
  purchase_total_cents: number;
  purchase_price_description: string;
  base_unit: BaseUnit;
  supplier: string;
  document_reference: string;
  note: string | null;
  quote_fingerprint: string;
}

export interface StockCountQuote {
  product_id: number;
  product_name: string;
  product_sku: string;
  base_unit: BaseUnit;
  stock_before: number;
  counted_stock: number;
  difference: number;
  type: InventoryMovementType;
  base_quantity: number;
  adjustment_reason: string;
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
  centimeter_stock: number;
}

export interface QuotePurchasePriceCorrectionInput {
  movement_id: number;
  new_purchase_unit_price: string;
}

export interface ConfirmPurchasePriceCorrectionInput
  extends QuotePurchasePriceCorrectionInput {
  quote_fingerprint: string;
}

export interface PurchasePriceCorrectionQuote {
  movement_id: number;
  product_id: number;
  product_name: string;
  product_sku: string;
  base_unit: BaseUnit;
  capture_name: string;
  presentation_id: number | null;
  presentation_name: string | null;
  presentation_mode: PresentationMode | null;
  presentation_base_quantity: number | null;
  entered_quantity: number;
  base_quantity: number;
  previous_purchase_unit_price_cents: number;
  new_purchase_unit_price_cents: number;
  previous_purchase_total_cents: number;
  new_purchase_total_cents: number;
  quote_fingerprint: string;
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
