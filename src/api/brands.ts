import { invoke } from "@tauri-apps/api/core";
import type { Brand, SaveBrandInput } from "../types/brand";

export function listBrands(): Promise<Brand[]> {
  return invoke<Brand[]>("list_brands");
}

export function createBrand(input: SaveBrandInput): Promise<Brand> {
  return invoke<Brand>("create_brand", { input });
}

export function updateBrand(id: number, input: SaveBrandInput): Promise<Brand> {
  return invoke<Brand>("update_brand", { id, input });
}
