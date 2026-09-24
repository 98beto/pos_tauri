import { beforeEach, describe, expect, it } from "vitest";
import { invokeMock } from "../test/setup";
import { currentUser, initializeOwner, isAuthenticated, login, logout, ownerExists } from "./auth";
import { createBrand, listBrands, updateBrand } from "./brands";
import { addCartItem, checkoutCart, clearCart, decrementCartItem, getCart, incrementCartItem, quoteCashChange, removeCartItem } from "./cart";
import { createCategory, listCategories, updateCategory } from "./categories";
import { createInventoryMovement, getInventoryView, listInventoryMovements } from "./inventory";
import { createProduct, findProductBySku, getProductsView, listProductSelector, listProducts, updateProduct } from "./products";
import { getSaleHistoryDetail, getSalesView, listSales } from "./sales";

beforeEach(() => invokeMock.mockReset().mockResolvedValue(undefined));

describe("Tauri API adapters", () => {
  it("maps auth and cart operations to command names and payloads", async () => {
    const credentials = { email: "ada@example.com", password: "secret" };
    const owner = { ...credentials, first_name: "Ada", last_name: "Lovelace", confirm_password: "secret" };
    const checkout = { checkout_token: "token", cart_fingerprint: "cart", payment_method: "tarjeta" as const };
    await Promise.all([
      ownerExists(), initializeOwner(owner), login(credentials), logout(), isAuthenticated(), currentUser(),
      getCart(), addCartItem(7), incrementCartItem(7), decrementCartItem(7), removeCartItem(7), clearCart(), quoteCashChange("20.00"), checkoutCart(checkout),
    ]);

    expect(invokeMock.mock.calls).toEqual([
      ["owner_exists"], ["initialize_owner", { input: owner }], ["login", { input: credentials }], ["logout"], ["is_authenticated"], ["current_user"],
      ["get_cart"], ["add_cart_item", { productId: 7 }], ["increment_cart_item", { productId: 7 }], ["decrement_cart_item", { productId: 7 }],
      ["remove_cart_item", { productId: 7 }], ["clear_cart"], ["quote_cash_change", { cashReceived: "20.00" }], ["checkout_cart", { input: checkout }],
    ]);
  });

  it("maps views and CRUD operations without changing backend field names", async () => {
    const product = { name: "Cafe", sku: "CAFE", price: "12.50", stock: 3, brand_id: 1, category_id: 2 };
    const productUpdate = { name: "Cafe", sku: "CAFE", price: "13.00", brand_id: 1, category_id: 2 };
    const movement = { product_id: 4, reason: "ajuste" as const, type: "salida" as const, quantity: "2", operation_token: "movement-token" };
    await Promise.all([
      getProductsView({ search: "caf", brand_id: 1, category_id: 2 }), listProductSelector({ search: "caf", category_id: 2 }),
      listProducts(), findProductBySku("CAFE"), createProduct(product), updateProduct(4, productUpdate),
      listBrands(), createBrand({ name: "Origen" }), updateBrand(1, { name: "Nuevo origen" }),
      listCategories(), createCategory({ name: "Bebidas" }), updateCategory(2, { name: "Calientes" }),
      getInventoryView({ search: "caf", reason: "ajuste", type: "salida" }), listInventoryMovements(), createInventoryMovement(movement),
      getSalesView({ search: "0001", payment_method: "efectivo" }), getSaleHistoryDetail(9), listSales(),
    ]);

    expect(invokeMock.mock.calls).toEqual([
      ["get_products_view", { filters: { search: "caf", brand_id: 1, category_id: 2 } }],
      ["list_product_selector", { filters: { search: "caf", category_id: 2 } }], ["list_products"], ["find_product_by_sku", { sku: "CAFE" }],
      ["create_product", { input: product }], ["update_product", { id: 4, input: productUpdate }],
      ["list_brands"], ["create_brand", { input: { name: "Origen" } }], ["update_brand", { id: 1, input: { name: "Nuevo origen" } }],
      ["list_categories"], ["create_category", { input: { name: "Bebidas" } }], ["update_category", { id: 2, input: { name: "Calientes" } }],
      ["get_inventory_view", { filters: { search: "caf", reason: "ajuste", type: "salida" } }], ["list_inventory_movements"],
      ["create_inventory_movement", { input: movement }],
      ["get_sales_view", { filters: { search: "0001", payment_method: "efectivo" } }],
      ["get_sale_history_detail", { saleId: 9 }], ["list_sales"],
    ]);
  });
});
