import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createInventoryMovement, getInventoryView } from "../../api/inventory";
import { listProducts } from "../../api/products";
import { deferred } from "../../test/fixtures";
import { user } from "../../test/fixtures";
import type { InventoryView as InventoryViewData } from "../../types/inventory_movement";
import type { Product } from "../../types/product";
import { InventoryView } from "./InventoryView";
import { savePendingManualMovement, type PendingManualMovement } from "./manual-movement-intent";

vi.mock("../../api/inventory", () => ({ createInventoryMovement: vi.fn(), getInventoryView: vi.fn() }));
vi.mock("../../api/products", () => ({ listProducts: vi.fn() }));

const mockedGetInventoryView = vi.mocked(getInventoryView);
const mockedCreateInventoryMovement = vi.mocked(createInventoryMovement);
const mockedListProducts = vi.mocked(listProducts);
const inventory: InventoryViewData = {
  stats: { global_total_entries: 9, global_total_exits: 3, global_current_stock: 6, global_out_of_stock_products: 1 },
  items: [{
    id: 1,
    product_id: 11,
    product_name: "Cafe molido",
    sku: "CAFE-1",
    sale_id: null,
    reason: "compra",
    type: "entrada",
    quantity: 9,
    ticket_reference: null,
    created_at: "2026-01-01T10:00:00Z",
    updated_at: "2026-01-01T10:00:00Z",
  }],
};
const products: Product[] = [{ id: 11, name: "Cafe molido", sku: "CAFE-1", stock: 6, price: 1200, brand_id: 1, category_id: 1, created_at: "2026-01-01", updated_at: "2026-01-01" }];

function pendingIntent(overrides: Partial<PendingManualMovement> = {}): PendingManualMovement {
  return {
    version: 1,
    userId: user.id,
    operationToken: "pending-token",
    productId: 11,
    reason: "ajuste",
    type: "salida",
    quantity: "02",
    ...overrides,
  };
}

function renderInventory(overrides: Partial<React.ComponentProps<typeof InventoryView>> = {}) {
  return render(<InventoryView user={user} onNotice={vi.fn()} onSessionRequired={vi.fn()} {...overrides} />);
}

async function openMovement() {
  fireEvent.click(screen.getByRole("button", { name: "Registrar movimiento" }));
  await act(async () => undefined);
}

beforeEach(() => {
  vi.useFakeTimers();
  mockedGetInventoryView.mockResolvedValue(inventory);
  mockedListProducts.mockResolvedValue(products);
  mockedCreateInventoryMovement.mockResolvedValue(inventory.items[0]);
});

describe("InventoryView", () => {
  it("shows loading state and backend statistics, then sends remote filters", async () => {
    const first = deferred<InventoryViewData>();
    mockedGetInventoryView.mockReturnValueOnce(first.promise).mockResolvedValue(inventory);
    renderInventory();
    expect(screen.getAllByText("-")).toHaveLength(4);
    expect(screen.getByLabelText("Movimientos de inventario desplazables").getAttribute("aria-busy")).toBe("true");

    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => first.resolve(inventory));
    expect(screen.getByText("Cafe molido")).toBeTruthy();
    expect(screen.getByText("9")).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Buscar producto"), { target: { value: "cafe" } });
    fireEvent.change(screen.getByLabelText("Motivo"), { target: { value: "compra" } });
    fireEvent.change(screen.getByLabelText("Tipo"), { target: { value: "entrada" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    expect(mockedGetInventoryView).toHaveBeenLastCalledWith({ search: "cafe", reason: "compra", type: "entrada" });
  });

  it("uses a safe error and supports retry", async () => {
    mockedGetInventoryView.mockRejectedValueOnce(new Error("SQLITE_IOERR /private/path" )).mockResolvedValueOnce(inventory);
    renderInventory();
    await act(() => vi.advanceTimersByTimeAsync(250));

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("No se pudo consultar el inventario.");
    expect(alert.textContent).not.toContain("SQLITE_IOERR");
    fireEvent.click(screen.getByRole("button", { name: "Reintentar" }));
    await act(async () => undefined);
    expect(screen.getByText("Cafe molido")).toBeTruthy();
  });

  it("ignores out-of-order list responses and completion after unmount", async () => {
    const initial = deferred<InventoryViewData>();
    const first = deferred<InventoryViewData>();
    const second = deferred<InventoryViewData>();
    mockedGetInventoryView.mockReturnValueOnce(initial.promise).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { unmount } = renderInventory();
    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => initial.resolve(inventory));
    fireEvent.change(screen.getByLabelText("Buscar producto"), { target: { value: "a" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.change(screen.getByLabelText("Buscar producto"), { target: { value: "b" } });
    await act(() => vi.advanceTimersByTimeAsync(250));

    await act(async () => second.resolve({ ...inventory, items: [{ ...inventory.items[0], product_name: "Resultado B" }] }));
    expect(screen.getByText("Resultado B")).toBeTruthy();
    await act(async () => first.resolve({ ...inventory, items: [{ ...inventory.items[0], product_name: "Resultado A" }] }));
    expect(screen.queryByText("Resultado A")).toBeNull();

    const afterUnmount = deferred<InventoryViewData>();
    mockedGetInventoryView.mockReturnValueOnce(afterUnmount.promise);
    fireEvent.change(screen.getByLabelText("Buscar producto"), { target: { value: "c" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    unmount();
    await act(async () => afterUnmount.resolve(inventory));
  });

  it("keeps global stats but marks the previous table while a filtered request fails", async () => {
    renderInventory();
    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => undefined);
    const request = deferred<InventoryViewData>();
    mockedGetInventoryView.mockReturnValueOnce(request.promise);
    fireEvent.change(screen.getByLabelText("Buscar producto"), { target: { value: "nuevo" } });
    expect(screen.getByText("Actualizando; se muestran temporalmente los resultados anteriores.")).toBeTruthy();
    expect(screen.getByText("9")).toBeTruthy();
    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => request.reject(new Error("SQLITE details")));
    expect(screen.getByText("Se muestran los resultados anteriores; no corresponden a los filtros actuales.")).toBeTruthy();
    expect(screen.getByText("Cafe molido")).toBeTruthy();
    expect(screen.queryByText("No hay movimientos con estos filtros")).toBeNull();
  });

  it.each([
    ["Compra", { product_id: 11, reason: "compra", type: "entrada", quantity: "2" }],
    ["Ajuste de entrada", { product_id: 11, reason: "ajuste", type: "entrada", quantity: "2" }],
    ["Ajuste de salida", { product_id: 11, reason: "ajuste", type: "salida", quantity: "2" }],
  ] as const)("submits the %s operation and shows informational stock", async (label, expected) => {
    renderInventory();
    await openMovement();

    expect(screen.getByText("Stock actual:").textContent).toContain("6");
    fireEvent.change(screen.getByLabelText("Operacion obligatorio"), { target: { value: label === "Compra" ? "purchase" : label === "Ajuste de entrada" ? "adjustment-in" : "adjustment-out" } });
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "2" } });
    fireEvent.click(screen.getByRole("button", { name: "Guardar movimiento" }));
    await act(async () => undefined);

    expect(mockedCreateInventoryMovement).toHaveBeenCalledWith({ ...expected, operation_token: expect.any(String) });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("sends quantity text to Rust and associates backend quantity errors with the field", async () => {
    mockedCreateInventoryMovement.mockRejectedValue({ category: "validation", code: "INVALID_MOVEMENT_QUANTITY", message: "Cantidad invalida", field: "quantity" });
    renderInventory();
    await openMovement();
    const quantity = screen.getByLabelText(/^Cantidad/);
    fireEvent.change(quantity, { target: { value: "1.5" } });
    fireEvent.submit(document.getElementById("inventory-movement-form")!);
    await act(async () => undefined);
    await act(() => vi.runAllTimersAsync());

    expect(mockedCreateInventoryMovement).toHaveBeenCalledWith(expect.objectContaining({ quantity: "1.5" }));
    expect(quantity.getAttribute("aria-invalid")).toBe("true");
    expect(quantity.getAttribute("aria-describedby")).toBeTruthy();
    expect(document.activeElement).toBe(quantity);
    const loadsBeforeDiscard = mockedGetInventoryView.mock.calls.length;
    fireEvent.click(screen.getByRole("button", { name: "Descartar intencion" }));
    expect(screen.getByRole("dialog", { name: "Descartar intencion" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Descartar y recargar" }));
    await act(async () => undefined);
    expect(mockedGetInventoryView.mock.calls.length).toBe(loadsBeforeDiscard + 1);
    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).toBeNull();
  });

  it("blocks controls and same-tick duplicate submissions while saving", async () => {
    const request = deferred<(typeof inventory.items)[number]>();
    mockedCreateInventoryMovement.mockReturnValue(request.promise);
    renderInventory();
    await openMovement();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "2" } });
    const form = document.getElementById("inventory-movement-form")!;

    fireEvent.submit(form);
    fireEvent.submit(form);
    expect(mockedCreateInventoryMovement).toHaveBeenCalledTimes(1);
    expect(screen.getByLabelText(/^Cantidad/).hasAttribute("disabled")).toBe(true);
    expect(screen.getByRole("button", { name: "Cerrar dialogo" }).hasAttribute("disabled")).toBe(true);

    await act(async () => request.resolve(inventory.items[0]));
  });

  it("persists the versioned intent before invoking the backend", async () => {
    mockedCreateInventoryMovement.mockImplementation(async (input) => {
      const stored = localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`);
      expect(stored).not.toBeNull();
      expect(JSON.parse(stored!).operationToken).toBe(input.operation_token);
      return inventory.items[0];
    });
    renderInventory();
    await openMovement();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "2" } });
    fireEvent.submit(document.getElementById("inventory-movement-form")!);
    await act(async () => undefined);

    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).toBeNull();
  });

  it("keeps an uncertain intent and retries the same token after remount", async () => {
    mockedCreateInventoryMovement.mockRejectedValueOnce(new Error("IPC response lost"));
    const first = renderInventory();
    await openMovement();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "2" } });
    fireEvent.submit(document.getElementById("inventory-movement-form")!);
    await act(async () => undefined);

    const firstInput = mockedCreateInventoryMovement.mock.calls[0][0];
    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).not.toBeNull();
    first.unmount();
    mockedCreateInventoryMovement.mockResolvedValueOnce(inventory.items[0]);
    renderInventory();
    fireEvent.click(screen.getByRole("button", { name: "Reintentar movimiento" }));
    await act(async () => undefined);

    expect(mockedCreateInventoryMovement).toHaveBeenCalledTimes(2);
    expect(mockedCreateInventoryMovement.mock.calls[1][0].operation_token).toBe(firstInput.operation_token);
    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).toBeNull();
  });

  it("hydrates the recovered form and loads products while retrying", async () => {
    savePendingManualMovement(pendingIntent());
    const request = deferred<(typeof inventory.items)[number]>();
    const productRequest = deferred<Product[]>();
    mockedCreateInventoryMovement.mockReturnValue(request.promise);
    mockedListProducts.mockReturnValue(productRequest.promise);
    renderInventory();

    fireEvent.click(screen.getByRole("button", { name: "Reintentar movimiento" }));

    expect(screen.getByRole("dialog", { name: "Registrar movimiento" })).toBeTruthy();
    expect((screen.getByLabelText(/^Cantidad/) as HTMLInputElement).value).toBe("02");
    expect((screen.getByLabelText("Operacion obligatorio") as HTMLSelectElement).value).toBe("adjustment-out");
    expect(mockedListProducts).toHaveBeenCalledOnce();
    expect(mockedCreateInventoryMovement).toHaveBeenCalledWith(expect.objectContaining({
      product_id: 11,
      quantity: "02",
      operation_token: "pending-token",
    }));

    await act(async () => productRequest.resolve(products));
    expect((screen.getByLabelText("Producto obligatorio") as HTMLSelectElement).value).toBe("11");
    await act(async () => request.reject({ category: "validation", code: "INVALID_MOVEMENT_QUANTITY", message: "Cantidad invalida", field: "quantity" }));
    await act(() => vi.runAllTimersAsync());
    expect(screen.getByLabelText(/^Cantidad/).getAttribute("aria-invalid")).toBe("true");
    expect(document.activeElement).toBe(screen.getByLabelText(/^Cantidad/));
  });

  it("joins an in-flight recovered request across unmount and remount", async () => {
    savePendingManualMovement(pendingIntent());
    const request = deferred<(typeof inventory.items)[number]>();
    mockedCreateInventoryMovement.mockReturnValue(request.promise);

    const first = renderInventory();
    fireEvent.click(screen.getByRole("button", { name: "Reintentar movimiento" }));
    first.unmount();
    renderInventory();
    fireEvent.click(screen.getByRole("button", { name: "Reintentar movimiento" }));

    expect(mockedCreateInventoryMovement).toHaveBeenCalledTimes(1);
    await act(async () => request.resolve(inventory.items[0]));
    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).toBeNull();
    expect(screen.queryByText("Movimiento pendiente de confirmar")).toBeNull();
  });

  it("keeps confirmed recovery visible until local cleanup is verified", async () => {
    const onNotice = vi.fn();
    const remove = vi.spyOn(Storage.prototype, "removeItem").mockImplementation(() => {
      throw new Error("storage unavailable");
    });
    renderInventory({ onNotice });
    await openMovement();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "2" } });
    fireEvent.submit(document.getElementById("inventory-movement-form")!);
    await act(async () => undefined);

    expect(screen.getByText("Movimiento confirmado con recuperacion local pendiente")).toBeTruthy();
    expect(screen.getByRole("dialog", { name: "Registrar movimiento" })).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("fue confirmado");
    expect(onNotice).toHaveBeenCalledWith(expect.objectContaining({ message: expect.stringContaining("fue confirmado") }));

    remove.mockRestore();
    fireEvent.click(screen.getByRole("button", { name: "Reintentar limpieza" }));
    await act(async () => undefined);
    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).toBeNull();
    expect(screen.queryByText("Movimiento confirmado con recuperacion local pendiente")).toBeNull();
  });

  it.each([
    ["another user", { userId: user.id + 1 }],
    ["another token", { operationToken: "replacement-token" }],
  ] as const)("does not report cleanup success or delete an intent for %s", async (_case, overrides) => {
    const original = pendingIntent();
    const replacement = pendingIntent(overrides);
    const key = `pos.inventory-movement-intent.user.${user.id}`;
    const request = deferred<(typeof inventory.items)[number]>();
    const onNotice = vi.fn();
    savePendingManualMovement(original);
    mockedCreateInventoryMovement.mockReturnValue(request.promise);
    renderInventory({ onNotice });

    fireEvent.click(screen.getByRole("button", { name: "Reintentar movimiento" }));
    localStorage.setItem(key, JSON.stringify(replacement));
    await act(async () => request.resolve(inventory.items[0]));

    expect(screen.getByText("Movimiento confirmado con recuperacion local pendiente")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("no se pudo limpiar");
    expect(onNotice).not.toHaveBeenCalledWith(expect.objectContaining({ tone: "success" }));

    fireEvent.click(screen.getByRole("button", { name: "Reintentar limpieza" }));
    await act(async () => undefined);

    expect(screen.getByText("Movimiento confirmado con recuperacion local pendiente")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("sigue confirmado");
    expect(onNotice).not.toHaveBeenCalledWith(expect.objectContaining({ tone: "success" }));
    expect(localStorage.getItem(key)).toBe(JSON.stringify(replacement));
  });

  it("keeps the discard dialog and pending banner when storage cannot be cleared", async () => {
    mockedCreateInventoryMovement.mockRejectedValue({
      category: "conflict",
      code: "INSUFFICIENT_STOCK",
      message: "Stock insuficiente",
    });
    renderInventory();
    await openMovement();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "99" } });
    fireEvent.submit(document.getElementById("inventory-movement-form")!);
    await act(async () => undefined);
    fireEvent.click(screen.getByRole("button", { name: "Descartar intencion" }));
    const remove = vi.spyOn(Storage.prototype, "removeItem").mockImplementation(() => {
      throw new Error("storage unavailable");
    });

    fireEvent.click(screen.getByRole("button", { name: "Descartar y recargar" }));
    await act(async () => undefined);

    expect(screen.getByRole("dialog", { name: "Descartar intencion" })).toBeTruthy();
    expect(screen.getByText("Movimiento pendiente de confirmar")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("se conservo");
    expect(localStorage.getItem(`pos.inventory-movement-intent.user.${user.id}`)).not.toBeNull();
    remove.mockRestore();
  });

  it("keeps backend errors open and delegates expired sessions", async () => {
    mockedCreateInventoryMovement.mockRejectedValueOnce({ category: "conflict", code: "INSUFFICIENT_STOCK", message: "Stock insuficiente" });
    const onSessionRequired = vi.fn();
    renderInventory({ onSessionRequired });
    await openMovement();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "99" } });
    fireEvent.submit(document.getElementById("inventory-movement-form")!);
    await act(async () => undefined);
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("Stock insuficiente");

    mockedCreateInventoryMovement.mockRejectedValueOnce({ category: "authentication", code: "SESSION_REQUIRED", message: "Sesion requerida" });
    fireEvent.submit(document.getElementById("inventory-movement-form")!);
    await act(async () => undefined);
    expect(onSessionRequired).toHaveBeenCalledOnce();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("preserves filters after save and reports refresh failure separately", async () => {
    const onNotice = vi.fn();
    renderInventory({ onNotice });
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.change(screen.getByLabelText("Buscar producto"), { target: { value: "cafe" } });
    fireEvent.change(screen.getByLabelText("Motivo"), { target: { value: "ajuste" } });
    fireEvent.change(screen.getByLabelText("Tipo"), { target: { value: "salida" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    mockedGetInventoryView.mockRejectedValueOnce(new Error("refresh failed"));

    await openMovement();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "1" } });
    fireEvent.submit(document.getElementById("inventory-movement-form")!);
    await act(async () => undefined);

    expect(mockedGetInventoryView).toHaveBeenLastCalledWith({ search: "cafe", reason: "ajuste", type: "salida" });
    expect(onNotice).toHaveBeenNthCalledWith(1, { tone: "success", message: "Movimiento de inventario registrado." });
    expect(onNotice).toHaveBeenNthCalledWith(2, expect.objectContaining({ tone: "info", message: expect.stringContaining("se guardo") }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("ignores stale product loading after the modal is closed and reopened", async () => {
    const first = deferred<Product[]>();
    const second = deferred<Product[]>();
    mockedListProducts.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    renderInventory();
    fireEvent.click(screen.getByRole("button", { name: "Registrar movimiento" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    fireEvent.click(screen.getByRole("button", { name: "Registrar movimiento" }));
    await act(async () => second.resolve([{ ...products[0], id: 22, name: "Producto nuevo" }]));
    await act(async () => first.resolve(products));

    expect(screen.getByRole("option", { name: "Producto nuevo - CAFE-1" })).toBeTruthy();
    expect(screen.queryByRole("option", { name: "Cafe molido - CAFE-1" })).toBeNull();
  });

  it("shows a safe product loading error and retries", async () => {
    mockedListProducts.mockRejectedValueOnce(new Error("SQLITE_IOERR /private/path")).mockResolvedValueOnce(products);
    renderInventory();
    await openMovement();

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("No se pudieron cargar los productos.");
    expect(alert.textContent).not.toContain("SQLITE_IOERR");
    expect(screen.getByRole("button", { name: "Guardar movimiento" }).hasAttribute("disabled")).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Reintentar" }));
    await act(async () => undefined);
    expect(screen.getByRole("option", { name: "Cafe molido - CAFE-1" })).toBeTruthy();
  });

  it("does not publish stale save outcomes after unmount", async () => {
    const request = deferred<(typeof inventory.items)[number]>();
    const onNotice = vi.fn();
    const onSessionRequired = vi.fn();
    mockedCreateInventoryMovement.mockReturnValue(request.promise);
    const { unmount } = renderInventory({ onNotice, onSessionRequired });
    await openMovement();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "2" } });
    fireEvent.submit(document.getElementById("inventory-movement-form")!);

    unmount();
    await act(async () => request.resolve(inventory.items[0]));

    expect(onNotice).not.toHaveBeenCalled();
    expect(onSessionRequired).not.toHaveBeenCalled();
    expect(mockedGetInventoryView).not.toHaveBeenCalled();
  });

  it("does not report session expiry during refresh as a retryable refresh failure", async () => {
    const onNotice = vi.fn();
    const onSessionRequired = vi.fn();
    mockedGetInventoryView.mockRejectedValueOnce({ category: "authentication", code: "SESSION_REQUIRED", message: "Sesion requerida" });
    renderInventory({ onNotice, onSessionRequired });
    await openMovement();
    fireEvent.change(screen.getByLabelText(/^Cantidad/), { target: { value: "1" } });
    fireEvent.submit(document.getElementById("inventory-movement-form")!);
    await act(async () => undefined);

    expect(onSessionRequired).toHaveBeenCalledOnce();
    expect(onNotice).toHaveBeenCalledTimes(1);
    expect(onNotice).toHaveBeenCalledWith({ tone: "success", message: "Movimiento de inventario registrado." });
  });
});
