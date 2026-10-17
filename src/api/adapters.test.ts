import { beforeEach, describe, expect, it } from "vitest";
import { invokeMock } from "../test/setup";
import { currentUser, initializeOwner, isAuthenticated, login, logout, ownerExists } from "./auth";
import { createBrand, listBrands, updateBrand } from "./brands";
import { addCartItem, checkoutCart, clearCart, decrementCartItem, getCart, incrementCartItem, quoteCartDiscount, quoteCashChange, removeCartDiscount, removeCartItem, resetCartItemUnitPrice, setCartDiscount, setCartItemQuantity, setCartItemUnitPrice } from "./cart";
import { createCategory, listCategories, updateCategory } from "./categories";
import { confirmPurchaseCostCorrection, createInventoryMovement, getInventoryView, getProductInventoryDetail, listInventoryMovements, quoteInventoryMovement, quotePurchaseCostCorrection, searchInventoryProducts } from "./inventory";
import { createProductWithPresentations, findProductByCode, findProductBySku, getProductsView, listProductSelector, listProducts, updateProductWithPresentations } from "./products";
import { getSaleHistoryDetail, getSalesView, listSales } from "./sales";

beforeEach(() => invokeMock.mockReset().mockResolvedValue(undefined));

describe("Tauri API adapters", () => {
  it("maps auth and cart operations to command names and payloads", async () => {
    const credentials = { email: "ada@example.com", password: "secret" };
    const owner = { ...credentials, first_name: "Ada", last_name: "Lovelace", confirm_password: "secret" };
    const checkout = { checkout_token: "token", cart_fingerprint: "cart", payment_method: "tarjeta" as const };
    await Promise.all([
      ownerExists(), initializeOwner(owner), login(credentials), logout(), isAuthenticated(), currentUser(),
      getCart(), addCartItem(7), incrementCartItem(7), setCartItemQuantity(7, 250), setCartItemUnitPrice(7, "12.50"), resetCartItemUnitPrice(7), quoteCartDiscount("percentage", "10.25"), setCartDiscount("percentage", "10.25"), removeCartDiscount(), decrementCartItem(7), removeCartItem(7), clearCart(), quoteCashChange("20.00"), checkoutCart(checkout),
    ]);

    expect(invokeMock.mock.calls).toEqual([
      ["owner_exists"], ["initialize_owner", { input: owner }], ["login", { input: credentials }], ["logout"], ["is_authenticated"], ["current_user"],
      ["get_cart"], ["add_cart_item", { presentationId: 7 }], ["increment_cart_item", { presentationId: 7 }], ["set_cart_item_quantity", { presentationId: 7, enteredQuantity: 250 }],
      ["set_cart_item_unit_price", { presentationId: 7, unitPrice: "12.50" }], ["reset_cart_item_unit_price", { presentationId: 7 }], ["quote_cart_discount", { discountType: "percentage", value: "10.25" }], ["set_cart_discount", { discountType: "percentage", value: "10.25" }], ["remove_cart_discount"], ["decrement_cart_item", { presentationId: 7 }],
      ["remove_cart_item", { presentationId: 7 }], ["clear_cart"], ["quote_cash_change", { cashReceived: "20.00" }], ["checkout_cart", { input: checkout }],
    ]);
  });

  it("maps views and CRUD operations without changing backend field names", async () => {
    const presentation = { name: "Kilogramo", mode: "measured" as const, base_quantity: 1000, price: "120.00", barcode: null, is_default: true, is_active: true };
    const productWithPresentations = { name: "Cafe", sku: "CAFE", brand_id: 1, category_id: 2, base_unit: "gram" as const, presentations: [presentation] };
    const productWithPresentationsUpdate = { name: "Cafe", sku: "CAFE", brand_id: 1, category_id: 2, base_unit: "gram" as const, presentations: [{ ...presentation, id: 8 }] };
    const quote = { product_id: 4, presentation_id: 8, reason: "compra" as const, type: "entrada" as const, quantity: "2", supplier: "Proveedor", document_reference: "FAC-1", note: "Recepcion" };
    const movement = { ...quote, operation_token: "movement-token", quote_fingerprint: "inventory-quote-v3:test" };
    const correctionQuote = { movement_id: 12, new_unit_price: "4.50" };
    const correctionConfirm = { ...correctionQuote, quote_fingerprint: "purchase-cost-correction:test" };
    await Promise.all([
      getProductsView({ search: "caf", brand_id: 1, category_id: 2 }), listProductSelector({ search: "caf", category_id: 2 }),
      listProducts(), findProductBySku("CAFE"), findProductByCode("750123"),
      createProductWithPresentations(productWithPresentations), updateProductWithPresentations(4, productWithPresentationsUpdate),
      listBrands(), createBrand({ name: "Origen" }), updateBrand(1, { name: "Nuevo origen" }),
      listCategories(), createCategory({ name: "Bebidas" }), updateCategory(2, { name: "Calientes" }),
      getInventoryView({ product_search: "caf", product_page: 2, base_unit: "gram", movement_search: "FAC-1", movement_page: 3, reason: "compra", type: "entrada" }),
      getProductInventoryDetail(4, { movement_search: "FAC-1", reason: "compra", type: "entrada", date_from: "2026-09-01", date_to: "2026-09-30", page: 2 }),
       listInventoryMovements(), searchInventoryProducts("caf", 2), quoteInventoryMovement(quote), createInventoryMovement(movement), quotePurchaseCostCorrection(correctionQuote), confirmPurchaseCostCorrection(correctionConfirm),
      getSalesView({ search: "0001", payment_method: "efectivo" }), getSaleHistoryDetail(9), listSales(),
    ]);

    expect(invokeMock.mock.calls).toEqual([
      ["get_products_view", { filters: { search: "caf", brand_id: 1, category_id: 2 } }],
      ["list_product_selector", { filters: { search: "caf", category_id: 2 } }], ["list_products"], ["find_product_by_sku", { sku: "CAFE" }], ["find_product_by_code", { code: "750123" }],
      ["create_product_with_presentations", { input: productWithPresentations }], ["update_product_with_presentations", { id: 4, input: productWithPresentationsUpdate }],
      ["list_brands"], ["create_brand", { input: { name: "Origen" } }], ["update_brand", { id: 1, input: { name: "Nuevo origen" } }],
      ["list_categories"], ["create_category", { input: { name: "Bebidas" } }], ["update_category", { id: 2, input: { name: "Calientes" } }],
      ["get_inventory_view", { filters: { product_search: "caf", product_page: 2, base_unit: "gram", movement_search: "FAC-1", movement_page: 3, reason: "compra", type: "entrada" } }],
        ["get_product_inventory_detail", { productId: 4, filters: { movement_search: "FAC-1", reason: "compra", type: "entrada", date_from: "2026-09-01", date_to: "2026-09-30", page: 2 } }], ["list_inventory_movements"], ["search_inventory_products", { search: "caf", page: 2 }], ["quote_inventory_movement", { input: quote }],
      ["create_inventory_movement", { input: movement }],
      ["quote_purchase_cost_correction", { input: correctionQuote }],
      ["confirm_purchase_cost_correction", { input: correctionConfirm }],
      ["get_sales_view", { filters: { search: "0001", payment_method: "efectivo" } }],
      ["get_sale_history_detail", { saleId: 9 }], ["list_sales"],
    ]);
  });
});
