export interface InventoryMovement {
  id: number;
  product_id: number;
  product_name: string;
  sku: string;
  sale_id: number | null;
  reason: "compra" | "venta" | "ajuste";
  type: "entrada" | "salida";
  quantity: number;
  created_at: string;
  updated_at: string;
}

export interface CreateInventoryMovementInput {
  product_id: number;
  reason: "compra" | "ajuste";
  type: "entrada" | "salida";
  quantity: string;
  operation_token: string;
}

export interface InventoryViewFilters {
  search?: string;
  reason?: InventoryMovement["reason"];
  type?: InventoryMovement["type"];
}

export interface InventoryViewRow extends InventoryMovement {
  ticket_reference: string | null;
}

export interface InventoryViewStats {
  global_total_entries: number;
  global_total_exits: number;
  global_current_stock: number;
  global_out_of_stock_products: number;
}

export interface InventoryView {
  items: InventoryViewRow[];
  stats: InventoryViewStats;
}
