import { invoke } from "@tauri-apps/api/core";
import type { Cart, CashChangeQuote, CheckoutCartInput } from "../types/cart";
import type { SaleReceipt } from "../types/sale";

export function getCart(): Promise<Cart> {
  return invoke<Cart>("get_cart");
}

export function addCartItem(productId: number): Promise<Cart> {
  return invoke<Cart>("add_cart_item", { productId });
}

export function incrementCartItem(productId: number): Promise<Cart> {
  return invoke<Cart>("increment_cart_item", { productId });
}

export function decrementCartItem(productId: number): Promise<Cart> {
  return invoke<Cart>("decrement_cart_item", { productId });
}

export function removeCartItem(productId: number): Promise<Cart> {
  return invoke<Cart>("remove_cart_item", { productId });
}

export function clearCart(): Promise<Cart> {
  return invoke<Cart>("clear_cart");
}

export function quoteCashChange(cashReceived: string): Promise<CashChangeQuote> {
  return invoke<CashChangeQuote>("quote_cash_change", { cashReceived });
}

export function checkoutCart(input: CheckoutCartInput): Promise<SaleReceipt> {
  return invoke<SaleReceipt>("checkout_cart", { input });
}
