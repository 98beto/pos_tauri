import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  addCartItem,
  checkoutCart,
  clearCart,
  decrementCartItem,
  getCart,
  incrementCartItem,
  quoteCashChange,
  removeCartItem,
} from "../../api/cart";
import { listCategories } from "../../api/categories";
import { listProductSelector } from "../../api/products";
import { cart, deferred, emptyCart, user } from "../../test/fixtures";
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
  quoteCashChange: vi.fn(),
  removeCartItem: vi.fn(),
}));
vi.mock("../../api/categories", () => ({ listCategories: vi.fn() }));
vi.mock("../../api/products", () => ({ listProductSelector: vi.fn() }));

const mockedGetCart = vi.mocked(getCart);
const mockedCheckoutCart = vi.mocked(checkoutCart);
const mockedQuoteCashChange = vi.mocked(quoteCashChange);
const receipt: SaleReceipt = {
  sale: {
    id: 3,
    user_id: user.id,
    cashier_name: "Ada Lovelace",
    cashier_email: user.email,
    checkout_token: "recover-token",
    cart_fingerprint: cart.cart_fingerprint,
    sale_date: "2026-01-01T10:00:00Z",
    total: cart.total,
    payment_method: "efectivo",
    cash_received: 2000,
    change_amount: 750,
    created_at: "2026-01-01",
    updated_at: "2026-01-01",
  },
  details: [],
};
const product = {
  id: 12,
  name: "Te verde",
  sku: "TE-1",
  price: 900,
  stock: 4,
  cart_quantity: 0,
  available_stock: 4,
  brand_id: 1,
  brand_name: "Origen",
  category_id: 2,
  category_name: "Bebidas",
};

function updaterControls(overrides: Partial<UpdaterControls> = {}): UpdaterControls {
  return {
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
  mockedQuoteCashChange.mockReset();
  mockedGetCart.mockResolvedValue(cart);
  vi.mocked(listCategories).mockResolvedValue([{ id: 2, name: "Bebidas", created_at: "", updated_at: "" }]);
  vi.mocked(listProductSelector).mockResolvedValue([product]);
  vi.mocked(addCartItem).mockResolvedValue(cart);
  vi.mocked(incrementCartItem).mockResolvedValue(cart);
  vi.mocked(decrementCartItem).mockResolvedValue(cart);
  vi.mocked(removeCartItem).mockResolvedValue(cart);
  vi.mocked(clearCart).mockResolvedValue(cart);
  mockedCheckoutCart.mockResolvedValue(receipt);
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
    vi.mocked(addCartItem).mockReturnValueOnce(request.promise);
    const reportCart = vi.fn<UpdaterControls["reportCart"]>();
    const view = render(
      <UpdaterProvider value={updaterControls({ reportCart })}>
        <PosView user={user} sessionId={41} onNotice={vi.fn()} onSessionRequired={vi.fn()} />
      </UpdaterProvider>,
    );
    await act(async () => undefined);
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Agregar · 4" }));
    view.unmount();

    await act(async () => request.resolve(cart));
    expect(reportCart).toHaveBeenLastCalledWith(41, { generation: 1, operation: 2 }, true);
    expect(reportCart).not.toHaveBeenCalledWith(42, expect.anything(), expect.anything());
  });

  it("suppresses late mutation callbacks after unmount", async () => {
    const request = deferred<typeof cart>();
    mockedGetCart.mockResolvedValueOnce(emptyCart);
    vi.mocked(addCartItem).mockReturnValueOnce(request.promise);
    const onNotice = vi.fn();
    const onSessionRequired = vi.fn();
    const view = await renderLoaded({ onNotice, onSessionRequired });
    fireEvent.click(screen.getByRole("button", { name: /Agregar producto/ }));
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.click(screen.getByRole("button", { name: "Agregar · 4" }));
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
    await renderLoaded();
    fireEvent.keyDown(window, { key: "F2" });
    expect(screen.getByRole("dialog", { name: "Agregar productos" })).toBeTruthy();
    await act(() => vi.advanceTimersByTimeAsync(250));
    await settleMutation(screen.getByRole("button", { name: "Agregar · 4" }));
    expect(addCartItem).toHaveBeenCalledWith(12);
    fireEvent.click(screen.getByRole("button", { name: "Cerrar dialogo" }));

    const cash = screen.getByLabelText(/^Efectivo recibido/);
    fireEvent.keyDown(cash, { key: "F2" });
    expect(screen.queryByRole("dialog", { name: "Agregar productos" })).toBeNull();

    await settleMutation(screen.getByRole("button", { name: "Aumentar Cafe molido" }));
    await settleMutation(screen.getByRole("button", { name: "Reducir Cafe molido" }));
    await settleMutation(screen.getByRole("button", { name: "Quitar Cafe molido" }));
    await settleMutation(screen.getByRole("button", { name: "Vaciar" }));
    expect(incrementCartItem).toHaveBeenCalledWith(11);
    expect(decrementCartItem).toHaveBeenCalledWith(11);
    expect(removeCartItem).toHaveBeenCalledWith(11);
    expect(clearCart).toHaveBeenCalledOnce();
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

  it("blocks cart mutations while recovering a pending checkout", async () => {
    const intent: PendingCheckout = { version: 1, userId: user.id, checkoutToken: "recover-token", cartFingerprint: cart.cart_fingerprint, paymentMethod: "tarjeta" };
    savePendingCheckout(intent);
    const request = deferred<SaleReceipt>();
    mockedCheckoutCart.mockReturnValue(request.promise);
    await renderLoaded();

    expect(screen.getByText("Cobro pendiente de confirmar")).toBeTruthy();
    const increment = screen.getByRole("button", { name: "Aumentar Cafe molido" }) as HTMLButtonElement;
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
});
