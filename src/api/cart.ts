import { invoke } from "@tauri-apps/api/core";
import type { Cart, CartDiscountQuote, CashChangeQuote, CheckoutCartInput } from "../types/cart";
import type { SaleReceipt } from "../types/sale";

export function getCart(): Promise<Cart> {
  return invoke<Cart>("get_cart");
}

export function addCartItem(presentationId: number): Promise<Cart> {
  return invoke<Cart>("add_cart_item", { presentationId });
}

export function incrementCartItem(presentationId: number): Promise<Cart> {
  return invoke<Cart>("increment_cart_item", { presentationId });
}

export function setCartItemQuantity(
  presentationId: number,
  enteredQuantity: number,
): Promise<Cart> {
  return invoke<Cart>("set_cart_item_quantity", { presentationId, enteredQuantity });
}

export function setCartItemUnitPrice(
  presentationId: number,
  unitPrice: string,
): Promise<Cart> {
  return invoke<Cart>("set_cart_item_unit_price", { presentationId, unitPrice });
}

export function resetCartItemUnitPrice(presentationId: number): Promise<Cart> {
  return invoke<Cart>("reset_cart_item_unit_price", { presentationId });
}

export function setCartDiscount(
  discountType: "percentage" | "fixed",
  value: string,
): Promise<Cart> {
  return invoke<Cart>("set_cart_discount", { discountType, value });
}

export function quoteCartDiscount(
  discountType: "percentage" | "fixed",
  value: string,
): Promise<CartDiscountQuote> {
  return invoke<CartDiscountQuote>("quote_cart_discount", { discountType, value });
}

export function removeCartDiscount(): Promise<Cart> {
  return invoke<Cart>("remove_cart_discount");
}

export function decrementCartItem(presentationId: number): Promise<Cart> {
  return invoke<Cart>("decrement_cart_item", { presentationId });
}

export function removeCartItem(presentationId: number): Promise<Cart> {
  return invoke<Cart>("remove_cart_item", { presentationId });
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
