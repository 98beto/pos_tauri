import { invoke } from "@tauri-apps/api/core";
import type { Category, SaveCategoryInput } from "../types/category";

export function listCategories(): Promise<Category[]> {
  return invoke<Category[]>("list_categories");
}

export function createCategory(input: SaveCategoryInput): Promise<Category> {
  return invoke<Category>("create_category", { input });
}

export function updateCategory(
  id: number,
  input: SaveCategoryInput,
): Promise<Category> {
  return invoke<Category>("update_category", { id, input });
}
