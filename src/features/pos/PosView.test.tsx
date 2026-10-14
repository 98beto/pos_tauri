import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  addCartItem,
  checkoutCart,
  clearCart,
  decrementCartItem,
  getCart,
  incrementCartItem,
  quoteCartDiscount,
  quoteCashChange,
  removeCartDiscount,
  removeCartItem,
  resetCartItemUnitPrice,
  setCartDiscount,
  setCartItemQuantity,
  setCartItemUnitPrice,
} from "../../api/cart";
import { listCategories } from "../../api/categories";
import { findProductByCode, listProductSelector } from "../../api/products";
import { cart as cartFixture, deferred, emptyCart as emptyCartFixture, user } from "../../test/fixtures";
import type { Cart } from "../../types/cart";
import type { ProductSelectorRow } from "../../types/product";
import type { SaleReceipt } from "../../types/sale";
import { savePendingCheckout, type PendingCheckout } from "./checkout-intent";
import { PosView } from "./PosView";
import { UpdaterProvider, type UpdaterControls } from "../updater/UpdaterContext";

vi.mock("../../api/cart", () => ({
  addCartItem: vi.fn(),
  checkoutCart: vi.fn(),
  clearCart: vi.fn(),
  decrementCartItem: vi.fn(),
  getCart: vi.fn(),
  incrementCartItem: vi.fn(),
  quoteCartDiscount: vi.fn(),
  quoteCashChange: vi.fn(),
  removeCartDiscount: vi.fn(),
  removeCartItem: vi.fn(),
  resetCartItemUnitPrice: vi.fn(),
  setCartDiscount: vi.fn(),
  setCartItemQuantity: vi.fn(),
  setCartItemUnitPrice: vi.fn(),
}));
vi.mock("../../api/categories", () => ({ listCategories: vi.fn() }));
vi.mock("../../api/products", () => ({ findProductByCode: vi.fn(), listProductSelector: vi.fn() }));

const mockedGetCart = vi.mocked(getCart);
const mockedCheckoutCart = vi.mocked(checkoutCart);
const mockedQuoteCartDiscount = vi.mocked(quoteCartDiscount);
const mockedQuoteCashChange = vi.mocked(quoteCashChange);
const emptyCart: Cart = { ...emptyCartFixture, discount_type: "none", discount_value: 0, discount_amount: 0 };
const cart: Cart = {
  ...cartFixture,
  discount_type: "none",
  discount_value: 0,
  discount_amount: 0,
  lines: cartFixture.lines.map((line) => {
    const price = "unit_price" in line ? line.unit_price as number : line.applied_unit_price;
    return { ...line, catalog_unit_price: price, applied_unit_price: price, price_overridden: false };
  }),
};
const receipt: SaleReceipt = {
  sale: {
    id: 3,
    user_id: user.id,
    cashier_name: "Ada Lovelace",
    cashier_email: user.email,
    checkout_token: "recover-token",
    cart_fingerprint: cart.cart_fingerprint,
    sale_date: "2026-01-01T10:00:00Z",
    subtotal: cart.total,
    discount_type: "none",
    discount_value: 0,
    discount_amount: 0,
    total: cart.total,
    payment_method: "efectivo",
    cash_received: 2000,
    change_amount: 750,
    created_at: "2026-01-01",
    updated_at: "2026-01-01",
  },
  details: [],
};
const fixedPresentation = {
  id: 12,
  product_id: 12,
  name: "Pieza",
  mode: "fixed" as const,
  base_quantity: 1,
  price: 900,
  barcode: "7501234567890",
  is_default: true,
  is_active: true,
  is_conversion_locked: false,
  created_at: "",
  updated_at: "",
};
const product: ProductSelectorRow = {
  id: 12,
  name: "Te verde",
  sku: "TE-1",
  price: 900,
  stock: 4,
  stock_base: 4,
  base_unit: "piece",
  cart_quantity: 0,
  available_stock: 4,
  brand_id: 1,
  brand_name: "Origen",
  category_id: 2,
  category_name: "Bebidas",
  presentations: [fixedPresentation],
  default_presentation: fixedPresentation,
};
const measuredPresentation = {
  ...fixedPresentation,
  id: 301,
  product_id: 30,
  name: "Kilogramo",
  mode: "measured" as const,
  base_quantity: 1_000,
  price: 12_000,
  barcode: "7500000000301",
};
const measuredProduct: ProductSelectorRow = {
  ...product,
  id: 30,
  name: "Cafe a granel",
  sku: "CAFE-G",
  stock: 2_000,
  stock_base: 2_000,
  base_unit: "gram",
  available_stock: 2_000,
  presentations: [measuredPresentation],
  default_presentation: measuredPresentation,
};
const measuredCart: Cart = {
  lines: [{
    presentation_id: measuredPresentation.id,
    presentation_name: measuredPresentation.name,
    presentation_barcode: measuredPresentation.barcode,
    product_id: measuredProduct.id,
    product_name: measuredProduct.name,
    sku: measuredProduct.sku,
    base_unit: "gram",
    mode: "measured",
    base_quantity: 1_000,
    catalog_unit_price: 12_000,
    applied_unit_price: 12_000,
    price_overridden: false,
    entered_quantity: 3,
    base_consumed: 3,
    stock_base: 2_000,
    available: true,
    line_total: 36,
  }],
  line_count: 1,
  subtotal: 36,
  discount_type: "none",
  discount_value: 0,
  discount_amount: 0,
  total: 36,
  cart_fingerprint: "cart-measured-3",
};

function updaterControls(overrides: Partial<UpdaterControls> = {}): UpdaterControls {
  return {
    enabled: true,
    manualCheck: vi.fn(),
    checking: false,
    busy: false,
    beginCartReporting: () => 1,
    beginCartOperation: () => true,
    reportCart: () => true,
    runCartMutation: (_sessionId, _version, operation) => operation(),
    runCheckout: (_sessionId, operation) => operation(),
    ...overrides,
  };
}

beforeEach(() => {
  vi.useFakeTimers();
  mockedGetCart.mockReset();
  mockedCheckoutCart.mockReset();
  mockedQuoteCartDiscount.mockReset();
  mockedQuoteCashChange.mockReset();
  mockedGetCart.mockResolvedValue(cart);
  vi.mocked(listCategories).mockResolvedValue([{ id: 2, name: "Bebidas", created_at: "", updated_at: "" }]);
  vi.mocked(listProductSelector).mockResolvedValue([product]);
  vi.mocked(findProductByCode).mockResolvedValue(null);
  vi.mocked(addCartItem).mockResolvedValue(cart);
  vi.mocked(incrementCartItem).mockResolvedValue(cart);
  vi.mocked(decrementCartItem).mockResolvedValue(cart);
  vi.mocked(removeCartItem).mockResolvedValue(cart);
  vi.mocked(removeCartDiscount).mockResolvedValue(cart);
  vi.mocked(resetCartItemUnitPrice).mockResolvedValue(cart);
  vi.mocked(setCartDiscount).mockResolvedValue(cart);
  vi.mocked(setCartItemQuantity).mockResolvedValue(measuredCart);
  vi.mocked(setCartItemUnitPrice).mockResolvedValue(cart);
  vi.mocked(clearCart).mockResolvedValue(cart);
  mockedCheckoutCart.mockResolvedValue(receipt);
  mockedQuoteCartDiscount.mockResolvedValue({ subtotal: 1_250, discount_type: "percentage", discount_value: 1_000, discount_amount: 125, total: 1_125 });
  mockedQuoteCashChange.mockResolvedValue({ total: cart.total, cash_received: 2000, change: 750 });
});

async function renderLoaded(options?: {
  onNotice?: (notice: { tone: "success" | "error" | "info"; message: string }) => void;
  onSessionRequired?: () => void;
  sessionId?: number;
}) {
  const onNotice = options?.onNotice ?? vi.fn();
  const onSessionRequired = options?.onSessionRequired ?? vi.fn();
  const view = render(<PosView user={user} sessionId={options?.sessionId ?? user.id} onNotice={onNotice} onSessionRequired={onSessionRequired} />);
  await act(async () => undefined);
  return { ...view, onNotice, onSessionRequired };
}

async function settleMutation(button: HTMLElement) {
  fireEvent.click(button);
  await act(async () => undefined);
}

async function enterValidCash(value = "20.00") {
  fireEvent.change(screen.getByLabelText(/^Efectivo recibido/), { target: { value } });
  await act(() => vi.advanceTimersByTimeAsync(250));
  await act(async () => undefined);
}

describe("PosView", () => {
  it("shows the cart line_count contract", async () => {
    mockedGetCart.mockResolvedValue({ ...cart, line_count: 7 });
    await renderLoaded();

    expect(screen.getByText("7 lineas en esta venta")).toBeTruthy();
    expect(screen.getByText("Lineas").nextElementSibling?.textContent).toBe("7");
    expect(screen.queryByText("7 articulos en esta venta")).toBeNull();
  });

  it("changes and resets a line price using the Rust cart contracts", async () => {
    const overridden: Cart = {
      ...cart,
      lines: [{ ...cart.lines[0], applied_unit_price: 1_100, price_overridden: true, line_total: 1_100 }],
      subtotal: 1_100,
      total: 1_100,
    };
    vi.mocked(setCartItemUnitPrice).mockResolvedValueOnce(overridden);
    vi.mocked(resetCartItemUnitPrice).mockResolvedValueOnce(cart);
    await renderLoaded();

    fireEvent.click(screen.getByRole("button", { name: "Cambiar precio de Cafe molido, Bolsa 250 g" }));
    expect(screen.getByText("Precio de catalogo").nextElementSibling?.textContent).toContain("$12.50");
    const input = screen.getByLabelText(/^Precio aplicado a esta venta/) as HTMLInputElement;
    expect(input.value).toBe("12.50");
    fireEvent.change(input, { target: { value: "11.00" } });
    await settleMutation(screen.getByRole("button", { name: "Guardar precio" }));

    expect(setCartItemUnitPrice).toHaveBeenCalledWith(101, "11.00");
    expect(screen.getByText(/Catalogo: \$12\.50 por Bolsa 250 g/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Cambiar precio de Cafe molido, Bolsa 250 g" }));
    await settleMutation(screen.getByRole("button", { name: "Restablecer precio de catalogo" }));
    expect(resetCartItemUnitPrice).toHaveBeenCalledWith(101);
  });

  it("shows price failures accessibly inside the modal", async () => {
    const onNotice = vi.fn();
    vi.mocked(setCartItemUnitPrice).mockRejectedValueOnce({ category: "validation", code: "INVALID_CART_UNIT_PRICE", message: "El precio no es valido" });
    await renderLoaded({ onNotice });
    fireEvent.click(screen.getByRole("button", { name: "Cambiar precio de Cafe molido, Bolsa 250 g" }));
    fireEvent.change(screen.getByLabelText(/^Precio aplicado a esta venta/), { target: { value: "0" } });
    await settleMutation(screen.getByRole("button", { name: "Guardar precio" }));

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("El precio no es valido");
    expect(screen.getByLabelText(/^Precio aplicado a esta venta/).getAttribute("aria-describedby")).toBe(alert.id);
    expect(onNotice).not.toHaveBeenCalled();
  });

  it("edits a measured price using its presentation price base", async () => {
    mockedGetCart.mockResolvedValueOnce(measuredCart);
    vi.mocked(setCartItemUnitPrice).mockResolvedValueOnce(measuredCart);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Cambiar precio de Cafe a granel, Kilogramo" }));

    expect(screen.getByText("Precio de catalogo").nextElementSibling?.textContent).toContain("$120.00 por kg");
    fireEvent.change(screen.getByLabelText(/^Precio aplicado a esta venta/), { target: { value: "100.00" } });
    await settleMutation(screen.getByRole("button", { name: "Guardar precio" }));
    expect(setCartItemUnitPrice).toHaveBeenCalledWith(301, "100.00");
  });

  it("previews discounts without mutating and only applies them on save", async () => {
    const discounted: Cart = {
      ...cart,
      discount_type: "percentage",
      discount_value: 1_000,
      discount_amount: 125,
      total: 1_125,
      cart_fingerprint: "discounted-cart",
    };
    mockedQuoteCartDiscount.mockResolvedValueOnce({ subtotal: 1_250, discount_type: "percentage", discount_value: 1_000, discount_amount: 125, total: 1_125 });
    vi.mocked(setCartDiscount).mockResolvedValueOnce(discounted);
    await renderLoaded();

    fireEvent.click(screen.getByRole("button", { name: "Aplicar descuento" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Porcentaje" }), { target: { value: "10.00" } });
    await settleMutation(screen.getByRole("button", { name: "Calcular vista previa" }));

    expect(mockedQuoteCartDiscount).toHaveBeenCalledWith("percentage", "10.00");
    expect(setCartDiscount).not.toHaveBeenCalled();
    expect(screen.getByText("Descuento 10%")).toBeTruthy();
    expect(screen.getByRole("dialog", { name: "Descuento de la venta" }).textContent).toContain("Total$11.25");
    await settleMutation(screen.getByRole("button", { name: "Guardar descuento" }));
    expect(setCartDiscount).toHaveBeenCalledWith("percentage", "10.00");
    expect(screen.queryByRole("dialog", { name: "Descuento de la venta" })).toBeNull();
  });

  it("cancels and unmounts discount previews without changing the cart", async () => {
    const lateQuote = deferred<Awaited<ReturnType<typeof quoteCartDiscount>>>();
    mockedQuoteCartDiscount.mockReturnValueOnce(lateQuote.promise);
    const view = await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Aplicar descuento" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Porcentaje" }), { target: { value: "10" } });
    fireEvent.click(screen.getByRole("button", { name: "Calcular vista previa" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(screen.queryByRole("dialog", { name: "Descuento de la venta" })).toBeNull();
    expect(setCartDiscount).not.toHaveBeenCalled();
    expect(removeCartDiscount).not.toHaveBeenCalled();

    view.unmount();
    await act(async () => lateQuote.resolve({ subtotal: 1_250, discount_type: "percentage", discount_value: 1_000, discount_amount: 125, total: 1_125 }));
    expect(setCartDiscount).not.toHaveBeenCalled();
  });

  it("previews a fixed discount and removes an existing discount explicitly", async () => {
    const discounted = { ...cart, discount_type: "fixed" as const, discount_value: 200, discount_amount: 200, total: 1_050 };
    mockedGetCart.mockResolvedValueOnce(discounted);
    mockedQuoteCartDiscount.mockResolvedValueOnce({ subtotal: 1_250, discount_type: "fixed", discount_value: 300, discount_amount: 300, total: 950 });
    vi.mocked(removeCartDiscount).mockResolvedValueOnce(cart);
    await renderLoaded();

    fireEvent.click(screen.getByRole("button", { name: "Cambiar descuento" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Importe" }), { target: { value: "3.00" } });
    await settleMutation(screen.getByRole("button", { name: "Calcular vista previa" }));
    expect(mockedQuoteCartDiscount).toHaveBeenCalledWith("fixed", "3.00");
    expect(screen.getByRole("dialog", { name: "Descuento de la venta" }).textContent).toContain("Total$9.50");
    expect(setCartDiscount).not.toHaveBeenCalled();
    await settleMutation(screen.getByRole("button", { name: "Quitar descuento" }));
    expect(removeCartDiscount).toHaveBeenCalledOnce();
  });

  it("shows discount quote and save failures accessibly inside the modal", async () => {
    mockedQuoteCartDiscount
      .mockRejectedValueOnce({ category: "validation", code: "INVALID_CART_DISCOUNT", message: "El descuento excede el subtotal" })
      .mockResolvedValueOnce({ subtotal: 1_250, discount_type: "percentage", discount_value: 1_000, discount_amount: 125, total: 1_125 });
    vi.mocked(setCartDiscount).mockRejectedValueOnce({ category: "conflict", code: "CART_CHANGED", message: "El total del carrito cambio" });
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Aplicar descuento" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Porcentaje" }), { target: { value: "99" } });
    await settleMutation(screen.getByRole("button", { name: "Calcular vista previa" }));
    expect(screen.getByRole("alert").textContent).toContain("El descuento excede el subtotal");

    fireEvent.change(screen.getByRole("textbox", { name: "Porcentaje" }), { target: { value: "10" } });
    await settleMutation(screen.getByRole("button", { name: "Calcular vista previa" }));
    await settleMutation(screen.getByRole("button", { name: "Guardar descuento" }));
    expect(screen.getByRole("alert").textContent).toContain("El total del carrito cambio");
    expect(screen.getByRole("dialog", { name: "Descuento de la venta" })).toBeTruthy();
  });

  it("reports a non-empty cart and pending checkout to the global updater", async () => {
    const intent: PendingCheckout = { version: 1, userId: user.id, checkoutToken: "updater-safety", cartFingerprint: cart.cart_fingerprint, paymentMethod: "tarjeta" };
    savePendingCheckout(intent);
    const checkout = deferred<SaleReceipt>();
    mockedCheckoutCart.mockReturnValue(checkout.promise);
    const reportCart = vi.fn<UpdaterControls["reportCart"]>();
    const runCheckoutCall = vi.fn();
    const runCheckout: UpdaterControls["runCheckout"] = (sessionId, operation) => {
      runCheckoutCall(sessionId, operation);
      return operation();
    };
    render(
      <UpdaterProvider value={updaterControls({ reportCart, runCheckout })}>
        <PosView user={user} sessionId={user.id} onNotice={vi.fn()} onSessionRequired={vi.fn()} />
      </UpdaterProvider>,
    );
    await act(async () => undefined);

    expect(reportCart).toHaveBeenCalledWith(user.id, { generation: 1, operation: 1 }, true);
    expect(runCheckoutCall).toHaveBeenCalledWith(user.id, expect.any(Function));
    await act(async () => checkout.resolve(receipt));
  });

  it("reports a cart mutation that resolves after navigation for its original session", async () => {
    const request = deferred<typeof cart>();
    mockedGetCart.mockResolvedValueOnce(emptyCart);
    vi.mocked(setCartItemQuantity).mockReturnValueOnce(request.promise);
    const reportCart = vi.fn<UpdaterControls["reportCart"]>();
    const view = render(
      <UpdaterProvider value={updaterControls({ reportCart })}>
        <PosView user={user} sessionId={41} onNotice={vi.fn()} onSessionRequired={vi.fn()} />
      </UpdaterProvider>,
    );
    await act(async () => undefined);
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Agregar Te verde, Pieza" }));
    fireEvent.change(screen.getByLabelText(/^Cantidad de piezas/), { target: { value: "2" } });
    fireEvent.click(screen.getByRole("button", { name: "Guardar cantidad" }));
    view.unmount();

    await act(async () => request.resolve(cart));
    expect(reportCart).toHaveBeenLastCalledWith(41, { generation: 1, operation: 2 }, true);
    expect(reportCart).not.toHaveBeenCalledWith(42, expect.anything(), expect.anything());
  });

  it("suppresses late mutation callbacks after unmount", async () => {
    const request = deferred<typeof cart>();
    mockedGetCart.mockResolvedValueOnce(emptyCart);
    vi.mocked(setCartItemQuantity).mockReturnValueOnce(request.promise);
    const onNotice = vi.fn();
    const onSessionRequired = vi.fn();
    const view = await renderLoaded({ onNotice, onSessionRequired });
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Agregar Te verde, Pieza" }));
    fireEvent.change(screen.getByLabelText(/^Cantidad de piezas/), { target: { value: "2" } });
    fireEvent.click(screen.getByRole("button", { name: "Guardar cantidad" }));
    view.unmount();

    await act(async () => request.reject({ category: "authentication", code: "SESSION_REQUIRED", message: "expired" }));
    expect(onNotice).not.toHaveBeenCalled();
    expect(onSessionRequired).not.toHaveBeenCalled();
  });

  it("suppresses late checkout callbacks but releases its global registration", async () => {
    const request = deferred<SaleReceipt>();
    mockedCheckoutCart.mockReturnValueOnce(request.promise);
    const onNotice = vi.fn();
    const onSessionRequired = vi.fn();
    let checkoutInFlight = false;
    const runCheckout: UpdaterControls["runCheckout"] = (_sessionId, operation) => {
      checkoutInFlight = true;
      return operation().finally(() => { checkoutInFlight = false; });
    };
    const view = render(
      <UpdaterProvider value={updaterControls({ reportCart: vi.fn(), runCheckout })}>
        <PosView user={user} sessionId={51} onNotice={onNotice} onSessionRequired={onSessionRequired} />
      </UpdaterProvider>,
    );
    await act(async () => undefined);
    await enterValidCash();
    fireEvent.click(screen.getByRole("button", { name: /Cobrar/ }));
    expect(checkoutInFlight).toBe(true);
    view.unmount();

    await act(async () => request.reject({ category: "authentication", code: "SESSION_REQUIRED", message: "expired" }));
    expect(checkoutInFlight).toBe(false);
    expect(onNotice).not.toHaveBeenCalled();
    expect(onSessionRequired).not.toHaveBeenCalled();
  });

  it("suppresses recovery notices when its cart refresh settles after unmount", async () => {
    savePendingCheckout({ version: 1, userId: user.id, checkoutToken: "recover-token", cartFingerprint: cart.cart_fingerprint, paymentMethod: "tarjeta" });
    const refresh = deferred<typeof emptyCart>();
    mockedGetCart.mockResolvedValueOnce(cart).mockReturnValueOnce(refresh.promise);
    const onNotice = vi.fn();
    const onSessionRequired = vi.fn();
    const view = await renderLoaded({ onNotice, onSessionRequired });
    await act(async () => undefined);
    view.unmount();

    await act(async () => refresh.reject(new Error("offline")));
    expect(onNotice).not.toHaveBeenCalled();
    expect(onSessionRequired).not.toHaveBeenCalled();
  });

  it("opens F2 only outside editing contexts and invokes every cart operation", async () => {
    vi.mocked(setCartItemQuantity).mockResolvedValue(cart);
    await renderLoaded();
    fireEvent.keyDown(window, { key: "F2" });
    expect(screen.getByRole("dialog", { name: "Agregar productos" })).toBeTruthy();
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Agregar Te verde, Pieza" }));
    expect(screen.queryByRole("dialog", { name: "Agregar productos" })).toBeNull();
    fireEvent.change(screen.getByLabelText(/^Cantidad de piezas/), { target: { value: "2" } });
    await settleMutation(screen.getByRole("button", { name: "Guardar cantidad" }));
    expect(setCartItemQuantity).toHaveBeenCalledWith(12, 2);
    expect(addCartItem).not.toHaveBeenCalled();

    const cash = screen.getByLabelText(/^Efectivo recibido/);
    fireEvent.keyDown(cash, { key: "F2" });
    expect(screen.queryByRole("dialog", { name: "Agregar productos" })).toBeNull();

    await settleMutation(screen.getByRole("button", { name: "Aumentar Cafe molido, Bolsa 250 g" }));
    await settleMutation(screen.getByRole("button", { name: "Reducir Cafe molido, Bolsa 250 g" }));
    await settleMutation(screen.getByRole("button", { name: "Quitar Cafe molido, Bolsa 250 g" }));
    await settleMutation(screen.getByRole("button", { name: "Vaciar" }));
    expect(incrementCartItem).toHaveBeenCalledWith(101);
    expect(decrementCartItem).toHaveBeenCalledWith(101);
    expect(removeCartItem).toHaveBeenCalledWith(101);
    expect(clearCart).toHaveBeenCalledOnce();
  });

  it("gives repeated cart and picker actions contextual accessible names", async () => {
    const cartPresentation = {
      ...fixedPresentation,
      id: 101,
      product_id: 11,
      name: "Bolsa 250 g",
      base_quantity: 250,
      price: 1_250,
      barcode: "750000000001",
    };
    const otherPresentation = { ...cartPresentation, id: 102, name: "Caja", barcode: null, is_default: false };
    const selectorProduct: ProductSelectorRow = {
      ...product,
      id: 11,
      name: "Cafe molido",
      sku: "CAFE-1",
      stock: 5,
      stock_base: 1_250,
      base_unit: "gram",
      cart_quantity: 1,
      available_stock: 1_000,
      presentations: [cartPresentation, otherPresentation],
      default_presentation: cartPresentation,
    };
    vi.mocked(listProductSelector).mockResolvedValue([selectorProduct]);
    await renderLoaded();

    expect(screen.getByRole("button", { name: "Editar cantidad de Cafe molido, Bolsa 250 g" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    await act(() => vi.advanceTimersByTimeAsync(250));
    expect(screen.getByRole("button", { name: "Editar Cafe molido, Bolsa 250 g" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Agregar Cafe molido, Caja" })).toBeTruthy();
  });

  it("shows active piece and bag presentations and captures each quantity by presentation id", async () => {
    const bag = { ...fixedPresentation, id: 13, name: "Bolsa 100", base_quantity: 100, price: 8_000, barcode: null, is_default: false };
    const inactive = { ...fixedPresentation, id: 14, name: "Caja descontinuada", is_active: false, is_default: false };
    vi.mocked(listProductSelector).mockResolvedValue([{ ...product, stock: 1_250, stock_base: 1_250, available_stock: 1_250, presentations: [fixedPresentation, bag, inactive] }]);
    vi.mocked(setCartItemQuantity).mockResolvedValue(emptyCart);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    await act(() => vi.advanceTimersByTimeAsync(250));

    expect(screen.getByText("Pieza")).toBeTruthy();
    expect(screen.getByText("Bolsa 100")).toBeTruthy();
    expect(screen.queryByText("Caja descontinuada")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Agregar Te verde, Pieza" }));
    fireEvent.change(screen.getByLabelText(/^Cantidad de piezas/), { target: { value: "25" } });
    await settleMutation(screen.getByRole("button", { name: "Guardar cantidad" }));
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Agregar Te verde, Bolsa 100" }));
    expect(screen.getByText(/12 bolsas 100 \/ 1,250 piezas/i)).toBeTruthy();
    fireEvent.change(screen.getByLabelText(/^Cantidad de bolsas 100/), { target: { value: "10" } });
    await settleMutation(screen.getByRole("button", { name: "Guardar cantidad" }));
    expect(setCartItemQuantity).toHaveBeenNthCalledWith(1, 12, 25);
    expect(setCartItemQuantity).toHaveBeenNthCalledWith(2, 13, 10);
    expect(addCartItem).not.toHaveBeenCalled();
  });

  it("sets a measured quantity of 250 with one IPC call", async () => {
    mockedGetCart.mockResolvedValueOnce(emptyCart);
    vi.mocked(listProductSelector).mockResolvedValue([measuredProduct]);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Agregar Cafe a granel, Kilogramo" }));

    const quantity = screen.getByLabelText(/^Cantidad en gramos/);
    expect(quantity.getAttribute("inputmode")).toBe("numeric");
    fireEvent.change(quantity, { target: { value: "250" } });
    fireEvent.keyDown(quantity, { key: "Enter" });
    await act(async () => undefined);
    expect(setCartItemQuantity).toHaveBeenCalledOnce();
    expect(setCartItemQuantity).toHaveBeenCalledWith(301, 250);
    expect(addCartItem).not.toHaveBeenCalled();
    expect(incrementCartItem).not.toHaveBeenCalled();
  });

  it("prefills and replaces an existing fixed quantity with one IPC call", async () => {
    mockedGetCart.mockResolvedValueOnce(cart);
    vi.mocked(setCartItemQuantity).mockResolvedValueOnce({
      ...cart,
      lines: [{ ...cart.lines[0], entered_quantity: 4, base_consumed: 1_000, line_total: 5_000 }],
      subtotal: 5_000,
      total: 5_000,
    });
    await renderLoaded();

    fireEvent.click(screen.getByRole("button", { name: "Editar cantidad de Cafe molido, Bolsa 250 g" }));
    const quantity = screen.getByLabelText(/^Cantidad de bolsas 250 g/) as HTMLInputElement;
    expect(quantity.value).toBe("1");
    expect(screen.getByText(/5 bolsas 250 g \/ 1.25 kg/i)).toBeTruthy();
    fireEvent.change(quantity, { target: { value: "4" } });
    await settleMutation(screen.getByRole("button", { name: "Guardar cantidad" }));

    expect(setCartItemQuantity).toHaveBeenCalledOnce();
    expect(setCartItemQuantity).toHaveBeenCalledWith(101, 4);
    expect(addCartItem).not.toHaveBeenCalled();
  });

  it("cancels fixed quantity capture without changing the cart", async () => {
    mockedGetCart.mockResolvedValueOnce(emptyCart);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Agregar Te verde, Pieza" }));
    fireEvent.change(screen.getByLabelText(/^Cantidad de piezas/), { target: { value: "3" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));

    expect(screen.queryByRole("dialog", { name: "Capturar cantidad" })).toBeNull();
    expect(setCartItemQuantity).not.toHaveBeenCalled();
  });

  it("validates aggregate availability across piece and bag presentations", async () => {
    const bag = { ...fixedPresentation, id: 13, name: "Bolsa 100", base_quantity: 100, barcode: null, is_default: false };
    const mixedCart: Cart = {
      ...emptyCart,
      lines: [{
        presentation_id: fixedPresentation.id,
        presentation_name: fixedPresentation.name,
        presentation_barcode: fixedPresentation.barcode,
        product_id: product.id,
        product_name: product.name,
        sku: product.sku,
        base_unit: "piece",
        mode: "fixed",
        base_quantity: 1,
        catalog_unit_price: fixedPresentation.price,
        applied_unit_price: fixedPresentation.price,
        price_overridden: false,
        entered_quantity: 25,
        base_consumed: 25,
        stock_base: 250,
        available: true,
        line_total: 22_500,
      }],
      line_count: 1,
      subtotal: 22_500,
      total: 22_500,
    };
    mockedGetCart.mockResolvedValueOnce(mixedCart);
    vi.mocked(listProductSelector).mockResolvedValueOnce([{ ...product, stock: 250, stock_base: 250, cart_quantity: 25, available_stock: 225, presentations: [fixedPresentation, bag] }]);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Agregar Te verde, Bolsa 100" }));
    fireEvent.change(screen.getByLabelText(/^Cantidad de bolsas 100/), { target: { value: "3" } });
    fireEvent.click(screen.getByRole("button", { name: "Guardar cantidad" }));

    expect(screen.getByRole("alert").textContent).toContain("Solo hay 2 bolsas 100 disponibles");
    expect(setCartItemQuantity).not.toHaveBeenCalled();
  });

  it("keeps the previous cart when setting quantity fails", async () => {
    const onNotice = vi.fn();
    vi.mocked(setCartItemQuantity).mockRejectedValueOnce(new Error("ipc failure"));
    await renderLoaded({ onNotice });
    fireEvent.click(screen.getByRole("button", { name: "Editar cantidad de Cafe molido, Bolsa 250 g" }));
    fireEvent.change(screen.getByLabelText(/^Cantidad de bolsas 250 g/), { target: { value: "4" } });
    await settleMutation(screen.getByRole("button", { name: "Guardar cantidad" }));

    expect(screen.getByRole("dialog", { name: "Editar cantidad" })).toBeTruthy();
    expect(screen.getByText(/1 Bolsa 250 g/)).toBeTruthy();
    const alert = screen.getByRole("alert");
    const quantity = screen.getByLabelText(/^Cantidad de bolsas 250 g/);
    expect(alert.textContent).toContain("No se pudo guardar la cantidad.");
    expect(quantity.getAttribute("aria-invalid")).toBe("true");
    expect(quantity.getAttribute("aria-describedby")).toBe(alert.id);
    expect(onNotice).not.toHaveBeenCalled();
  });

  it.each(["", "0", "1.5", "+2", "-2"])("rejects invalid measured quantity %j", async (value) => {
    mockedGetCart.mockResolvedValueOnce(measuredCart);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Editar cantidad de Cafe a granel, Kilogramo" }));
    fireEvent.change(screen.getByLabelText(/^Cantidad en gramos/), { target: { value } });
    fireEvent.click(screen.getByRole("button", { name: "Guardar cantidad" }));
    const alert = screen.getByRole("alert");
    const quantity = screen.getByLabelText(/^Cantidad en gramos/);
    expect(alert.textContent).toContain("entera mayor que cero");
    expect(quantity.getAttribute("aria-invalid")).toBe("true");
    expect(quantity.getAttribute("aria-describedby")).toBe(alert.id);
    expect(incrementCartItem).not.toHaveBeenCalled();
    expect(decrementCartItem).not.toHaveBeenCalled();
    expect(setCartItemQuantity).not.toHaveBeenCalled();
  });

  it("rejects a measured quantity above the available base stock", async () => {
    mockedGetCart.mockResolvedValueOnce(measuredCart);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Editar cantidad de Cafe a granel, Kilogramo" }));
    fireEvent.change(screen.getByLabelText(/^Cantidad en gramos/), { target: { value: "2001" } });
    fireEvent.click(screen.getByRole("button", { name: "Guardar cantidad" }));

    expect(screen.getByRole("alert").textContent).toContain("Solo hay 2 kg disponibles");
    expect(incrementCartItem).not.toHaveBeenCalled();
    expect(decrementCartItem).not.toHaveBeenCalled();
  });

  it("sets an edited measured quantity of 1000 with one IPC call", async () => {
    mockedGetCart.mockResolvedValueOnce(measuredCart);
    await renderLoaded();
    expect(screen.queryByRole("button", { name: /Aumentar Cafe a granel/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Editar cantidad de Cafe a granel, Kilogramo" }));
    fireEvent.change(screen.getByLabelText(/^Cantidad en gramos/), { target: { value: "1000" } });
    await settleMutation(screen.getByRole("button", { name: "Guardar cantidad" }));
    expect(setCartItemQuantity).toHaveBeenCalledOnce();
    expect(setCartItemQuantity).toHaveBeenCalledWith(301, 1000);
    expect(incrementCartItem).not.toHaveBeenCalled();
    expect(decrementCartItem).not.toHaveBeenCalled();
  });

  it("uses an exact barcode match and highlights only its presentation", async () => {
    const otherPresentation = { ...fixedPresentation, id: 13, name: "Caja", barcode: "OTRO", is_default: false };
    const selectorProduct = { ...product, presentations: [fixedPresentation, otherPresentation] };
    vi.mocked(listProductSelector).mockResolvedValue([selectorProduct]);
    vi.mocked(findProductByCode).mockResolvedValue({
      match_kind: "barcode",
      presentation_id: fixedPresentation.id,
      product: {
        ...product,
        inventory_cost_cents: null,
        inventory_cost_status: "unknown",
        average_cost_cents: null,
        created_at: "",
        updated_at: "",
        presentations: selectorProduct.presentations,
      },
    });
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    fireEvent.change(screen.getByLabelText("Buscar"), { target: { value: "7501234567890" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    expect(findProductByCode).toHaveBeenLastCalledWith("7501234567890");
    expect(listProductSelector).toHaveBeenLastCalledWith({ search: "7501234567890", category_id: undefined });
    expect(screen.getByText("Codigo exacto")).toBeTruthy();
    expect(screen.getByText("Codigo 7501234567890")).toBeTruthy();
    expect(screen.queryByText("Caja")).toBeNull();
  });

  it("shows every presentation for an exact SKU match", async () => {
    const otherPresentation = { ...fixedPresentation, id: 13, name: "Caja", barcode: "OTRO", is_default: false };
    const selectorProduct = { ...product, presentations: [fixedPresentation, otherPresentation] };
    vi.mocked(listProductSelector).mockResolvedValue([selectorProduct]);
    vi.mocked(findProductByCode).mockResolvedValue({
      match_kind: "sku",
      product: {
        ...product,
        inventory_cost_cents: null,
        inventory_cost_status: "unknown",
        average_cost_cents: null,
        created_at: "",
        updated_at: "",
        presentations: selectorProduct.presentations,
      },
    });
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    fireEvent.change(screen.getByLabelText("Buscar"), { target: { value: "TE-1" } });
    await act(() => vi.advanceTimersByTimeAsync(250));

    expect(screen.getByText("Pieza")).toBeTruthy();
    expect(screen.getByText("Caja")).toBeTruthy();
  });

  it("stores checkout first, invokes checkout, and blocks duplicate checkout", async () => {
    const request = deferred<SaleReceipt>();
    mockedCheckoutCart.mockReturnValue(request.promise);
    await renderLoaded();
    await enterValidCash();
    const checkout = screen.getByRole("button", { name: /Cobrar/ });
    fireEvent.click(checkout);
    fireEvent.click(checkout);

    expect(mockedCheckoutCart).toHaveBeenCalledOnce();
    expect(mockedCheckoutCart).toHaveBeenCalledWith(expect.objectContaining({
      cart_fingerprint: cart.cart_fingerprint,
      payment_method: "efectivo",
      cash_received: "20.00",
    }));
    expect(localStorage.getItem(`pos.checkout-intent.user.${user.id}`)).not.toBeNull();
    expect((checkout as HTMLButtonElement).disabled).toBe(true);
    request.resolve(receipt);
    await act(async () => request.promise);
  });

  it("renders receipt snapshots, entered quantity, and Rust line total", async () => {
    mockedCheckoutCart.mockResolvedValueOnce({
      ...receipt,
      details: [{
        id: 91,
        product_id: 11,
        sale_id: receipt.sale.id,
        record_version: 2,
        presentation_id: 101,
        product_name: "Cafe historico",
        sku: "CAFE-OLD",
        presentation_name: "Bolsa historica 250 g",
        presentation_barcode: null,
        base_unit: "gram",
        presentation_mode: "fixed",
        presentation_base_quantity: 250,
        quantity: 99,
        catalog_unit_price: 1_250,
        unit_price: 1_250,
        entered_quantity: 2,
        base_quantity: 500,
        line_total: 2_500,
        movement_cost_cents: 1_000,
        created_at: "2026-01-01",
        updated_at: "2026-01-01",
      }],
    });
    await renderLoaded();
    await enterValidCash();
    await settleMutation(screen.getByRole("button", { name: /Cobrar/ }));

    expect(screen.getByText("Cafe historico")).toBeTruthy();
    expect(screen.getByText("Bolsa historica 250 g · CAFE-OLD")).toBeTruthy();
    expect(screen.getByText(/2 Bolsa historica 250 g/)).toBeTruthy();
    expect(screen.getByText("$25.00")).toBeTruthy();
  });

  it("renders a legacy NULL line total without inventing an amount", async () => {
    mockedCheckoutCart.mockResolvedValueOnce({
      ...receipt,
      details: [{
        id: 92, product_id: 11, sale_id: receipt.sale.id, record_version: 1,
        presentation_id: null, product_name: "Linea legacy", sku: "OLD",
        presentation_name: null, presentation_barcode: null, base_unit: null,
        presentation_mode: null, presentation_base_quantity: null, quantity: 2,
        catalog_unit_price: null, unit_price: 625, entered_quantity: null,
        base_quantity: null, line_total: null, movement_cost_cents: null,
        created_at: "2026-01-01", updated_at: "2026-01-01",
      }],
    });
    await renderLoaded();
    await enterValidCash();
    await settleMutation(screen.getByRole("button", { name: /Cobrar/ }));
    expect(screen.getByText("No representable")).toBeTruthy();
  });

  it("labels the horizontally scrollable cart table", async () => {
    await renderLoaded();
    const region = screen.getByRole("region", { name: /Productos del carrito/ });
    expect(region.getAttribute("tabindex")).toBe("0");
    expect(screen.getByText("Bolsa 250 g · CAFE-1")).toBeTruthy();
    expect(screen.getAllByText("$12.50")).toHaveLength(3);
  });

  it("blocks cart mutations while recovering a pending checkout", async () => {
    const intent: PendingCheckout = { version: 1, userId: user.id, checkoutToken: "recover-token", cartFingerprint: cart.cart_fingerprint, paymentMethod: "tarjeta" };
    savePendingCheckout(intent);
    const request = deferred<SaleReceipt>();
    mockedCheckoutCart.mockReturnValue(request.promise);
    await renderLoaded();

    expect(screen.getByText("Cobro pendiente de confirmar")).toBeTruthy();
    const increment = screen.getByRole("button", { name: "Aumentar Cafe molido, Bolsa 250 g" }) as HTMLButtonElement;
    expect(increment.disabled).toBe(true);
    fireEvent.click(increment);
    expect(incrementCartItem).not.toHaveBeenCalled();
    request.resolve(receipt);
    await act(async () => request.promise);
  });

  it("recovers successfully and removes the stored intention", async () => {
    savePendingCheckout({ version: 1, userId: user.id, checkoutToken: "recover-token", cartFingerprint: cart.cart_fingerprint, paymentMethod: "tarjeta" });
    mockedGetCart.mockResolvedValueOnce(cart).mockResolvedValueOnce(emptyCart);
    const onNotice = vi.fn();
    await renderLoaded({ onNotice });
    await act(async () => undefined);

    expect(mockedCheckoutCart).toHaveBeenCalledWith({ checkout_token: "recover-token", cart_fingerprint: cart.cart_fingerprint, payment_method: "tarjeta" });
    expect(localStorage.getItem(`pos.checkout-intent.user.${user.id}`)).toBeNull();
    expect(onNotice).toHaveBeenCalledWith({ tone: "success", message: "Cobro recuperado correctamente." });
    expect(screen.getByRole("dialog", { name: "Venta completada" })).toBeTruthy();
  });

  it("keeps a confirmed checkout completed when refreshing the cart fails", async () => {
    mockedGetCart.mockResolvedValueOnce(cart).mockRejectedValueOnce(new Error("offline"));
    const onNotice = vi.fn();
    await renderLoaded({ onNotice });
    await enterValidCash();
    await settleMutation(screen.getByRole("button", { name: /Cobrar/ }));

    expect(screen.getByRole("dialog", { name: "Venta completada" })).toBeTruthy();
    expect(screen.queryByText("Cafe molido")).toBeNull();
    expect(screen.queryByText("Cobro pendiente de confirmar")).toBeNull();
    expect(localStorage.getItem(`pos.checkout-intent.user.${user.id}`)).toBeNull();
    expect(onNotice).toHaveBeenCalledWith({ tone: "error", message: "Venta completada, no se pudo actualizar el carrito." });
    expect(screen.queryByText(/confirmar el cobro/i)).toBeNull();
  });

  it("keeps a recovered checkout completed when refreshing the cart fails", async () => {
    savePendingCheckout({ version: 1, userId: user.id, checkoutToken: "recover-token", cartFingerprint: cart.cart_fingerprint, paymentMethod: "tarjeta" });
    mockedGetCart.mockResolvedValueOnce(cart).mockRejectedValueOnce(new Error("offline"));
    const onNotice = vi.fn();
    await renderLoaded({ onNotice });
    await act(async () => undefined);

    expect(screen.getByRole("dialog", { name: "Venta completada" })).toBeTruthy();
    expect(screen.queryByText("Cafe molido")).toBeNull();
    expect(screen.queryByText("Cobro pendiente de confirmar")).toBeNull();
    expect(localStorage.getItem(`pos.checkout-intent.user.${user.id}`)).toBeNull();
    expect(onNotice).toHaveBeenCalledWith({ tone: "error", message: "Venta completada, no se pudo actualizar el carrito." });
  });

  it("ignores stale picker responses and responses after unmount", async () => {
    const first = deferred<(typeof product)[]>();
    const second = deferred<(typeof product)[]>();
    vi.mocked(listProductSelector).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { unmount } = render(<PosView user={user} sessionId={user.id} onNotice={vi.fn()} onSessionRequired={vi.fn()} />);
    await act(async () => undefined);
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.change(screen.getByLabelText("Buscar"), { target: { value: "nuevo" } });
    await act(() => vi.advanceTimersByTimeAsync(250));

    await act(async () => second.resolve([{ ...product, id: 13, name: "Resultado B" }]));
    expect(screen.getByText("Resultado B")).toBeTruthy();
    await act(async () => first.resolve([{ ...product, name: "Resultado A" }]));
    expect(screen.queryByText("Resultado A")).toBeNull();

    fireEvent.change(screen.getByLabelText("Buscar"), { target: { value: "desmontar" } });
    const afterUnmount = deferred<(typeof product)[]>();
    vi.mocked(listProductSelector).mockReturnValueOnce(afterUnmount.promise);
    await act(() => vi.advanceTimersByTimeAsync(250));
    unmount();
    await act(async () => afterUnmount.resolve([product]));
  });

  it("distinguishes picker load errors from an empty successful result and retries", async () => {
    vi.mocked(listProductSelector).mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce([]);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    await act(() => vi.advanceTimersByTimeAsync(250));

    expect(screen.getByRole("alert").textContent).toContain("No se pudo cargar el catalogo");
    fireEvent.click(screen.getByRole("button", { name: "Reintentar" }));
    await act(async () => undefined);
    expect(screen.getByText("No hay productos disponibles")).toBeTruthy();
    expect(screen.queryByText("No se pudo cargar el catalogo")).toBeNull();
  });

  it("keeps previous picker results stale after refresh failure and retries", async () => {
    vi.mocked(listProductSelector).mockResolvedValueOnce([product]).mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce([product]);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    await act(() => vi.advanceTimersByTimeAsync(250));
    expect(screen.getByText("Te verde")).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Buscar"), { target: { value: "nuevo" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    expect(screen.getByText(/Se muestran resultados anteriores/)).toBeTruthy();
    expect(screen.getByText("Te verde")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Reintentar" }));
    await act(async () => undefined);
    expect(screen.queryByText(/Se muestran resultados anteriores/)).toBeNull();
  });

  it("keeps an uncertain recovery and hides technical error text", async () => {
    savePendingCheckout({ version: 1, userId: user.id, checkoutToken: "recover-token", cartFingerprint: cart.cart_fingerprint, paymentMethod: "tarjeta" });
    mockedCheckoutCart.mockRejectedValue(new Error("transport ipc channel stack"));
    await renderLoaded();
    await act(async () => undefined);

    expect(screen.getByText("Cobro pendiente de confirmar")).toBeTruthy();
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("No fue posible confirmar el estado del cobro. Usa Reintentar cobro.");
    expect(alert.textContent).not.toContain("ipc channel");
    expect(localStorage.getItem(`pos.checkout-intent.user.${user.id}`)).not.toBeNull();
  });

  it("quotes cash in Rust, ignores stale responses, and shows a safe technical error", async () => {
    const first = deferred<{ total: number; cash_received: number; change: number }>();
    const second = deferred<{ total: number; cash_received: number; change: number }>();
    mockedQuoteCashChange.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    await renderLoaded();
    fireEvent.change(screen.getByLabelText(/^Efectivo recibido/), { target: { value: "20.00" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.change(screen.getByLabelText(/^Efectivo recibido/), { target: { value: "30.00" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => second.resolve({ total: 1250, cash_received: 3000, change: 1750 }));
    expect(screen.getByText("$17.50")).toBeTruthy();
    await act(async () => first.resolve({ total: 1250, cash_received: 2000, change: 750 }));
    expect(screen.queryByText("$7.50")).toBeNull();

    mockedQuoteCashChange.mockRejectedValueOnce(new Error("ipc stack secret"));
    fireEvent.change(screen.getByLabelText(/^Efectivo recibido/), { target: { value: "40.00" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => undefined);
    expect(screen.getByText("No se pudo calcular el cambio de forma segura.")).toBeTruthy();
    expect(screen.queryByText(/ipc stack/)).toBeNull();
  });

  it("blocks the initial POS on load error and retries without showing false totals", async () => {
    mockedGetCart.mockRejectedValueOnce(new Error("database path")).mockResolvedValueOnce(cart);
    await renderLoaded();
    expect(screen.getByRole("heading", { name: "No se pudo cargar el punto de venta" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Cobrar/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Reintentar" }));
    await act(async () => undefined);
    expect(screen.getByRole("heading", { name: "Carrito actual" })).toBeTruthy();
  });

  it("keeps a changed-cart intention until accessible confirmation discards it", async () => {
    savePendingCheckout({ version: 1, userId: user.id, checkoutToken: "old-price", cartFingerprint: "cart-v2|11:1:1000", paymentMethod: "tarjeta" });
    await renderLoaded();
    expect(mockedCheckoutCart).not.toHaveBeenCalled();
    expect(screen.getByText(/El carrito cambio desde/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Descartar intencion" }));
    const dialog = screen.getByRole("dialog", { name: "Descartar intencion de cobro" });
    expect(dialog).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Descartar y recargar carrito" }));
    await act(async () => undefined);
    expect(localStorage.getItem(`pos.checkout-intent.user.${user.id}`)).toBeNull();
    expect(mockedGetCart).toHaveBeenCalledTimes(2);
  });

  it("keeps payment radios keyboard-focusable with a visible focus style", async () => {
    await renderLoaded();
    const card = screen.getByRole("radio", { name: "Tarjeta" });
    expect(card.getAttribute("class")).toContain("sr-only");
    expect(card.closest("label")?.className).toContain("has-[:focus-visible]:outline-2");
    card.focus();
    expect(document.activeElement).toBe(card);
  });

  it("keeps discount radios keyboard-focusable with a visible focus style", async () => {
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Aplicar descuento" }));
    const fixed = screen.getByRole("radio", { name: "Importe fijo" });
    expect(fixed.closest("label")?.className).toContain("has-[:focus-visible]:outline-2");
    fixed.focus();
    expect(document.activeElement).toBe(fixed);
  });
});
