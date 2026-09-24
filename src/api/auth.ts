import { invoke } from "@tauri-apps/api/core";
import type { InitializeOwnerInput, LoginInput, User } from "../types/user";

export function ownerExists(): Promise<boolean> {
  return invoke<boolean>("owner_exists");
}

export function initializeOwner(input: InitializeOwnerInput): Promise<User> {
  return invoke<User>("initialize_owner", { input });
}

export function login(input: LoginInput): Promise<User> {
  return invoke<User>("login", { input });
}

export function logout(): Promise<void> {
  return invoke<void>("logout");
}

export function isAuthenticated(): Promise<boolean> {
  return invoke<boolean>("is_authenticated");
}

export function currentUser(): Promise<User> {
  return invoke<User>("current_user");
}
