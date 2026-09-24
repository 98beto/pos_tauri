import { invoke } from "@tauri-apps/api/core";
import type {
  CreateInventoryMovementInput,
  InventoryMovement,
  InventoryView,
  InventoryViewFilters,
} from "../types/inventory_movement";

export function createInventoryMovement(
  input: CreateInventoryMovementInput,
): Promise<InventoryMovement> {
  return invoke<InventoryMovement>("create_inventory_movement", { input });
}

export function getInventoryView(
  filters: InventoryViewFilters = {},
): Promise<InventoryView> {
  return invoke<InventoryView>("get_inventory_view", { filters });
}

export function listInventoryMovements(): Promise<InventoryMovement[]> {
  return invoke<InventoryMovement[]>("list_inventory_movements");
}
