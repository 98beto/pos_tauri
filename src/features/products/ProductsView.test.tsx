import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createBrand, listBrands, updateBrand } from "../../api/brands";
import { createCategory, listCategories, updateCategory } from "../../api/categories";
import { createProductWithPresentations, getProductsView, updateProductWithPresentations } from "../../api/products";
import { deferred } from "../../test/fixtures";
import type { ProductView } from "../../types/product";
import { ProductsView } from "./ProductsView";

vi.mock("../../api/brands", () => ({ createBrand: vi.fn(), listBrands: vi.fn(), updateBrand: vi.fn() }));
vi.mock("../../api/categories", () => ({ createCategory: vi.fn(), listCategories: vi.fn(), updateCategory: vi.fn() }));
vi.mock("../../api/products", () => ({ createProductWithPresentations: vi.fn(), getProductsView: vi.fn(), updateProductWithPresentations: vi.fn() }));

const mockedListBrands = vi.mocked(listBrands);
const mockedListCategories = vi.mocked(listCategories);
const mockedGetProductsView = vi.mocked(getProductsView);
const mockedCreateProduct = vi.mocked(createProductWithPresentations);
const mockedUpdateProduct = vi.mocked(updateProductWithPresentations);

const piecePresentation = {
  id: 101,
  product_id: 11,
  name: "Bolsa 250 g",
  mode: "fixed" as const,
  base_quantity: 250,
  price: 1250,
  barcode: "750000000001",
  is_default: true,
  is_active: true,
  is_conversion_locked: true,
  created_at: "2026-01-01",
  updated_at: "2026-01-01",
};

const measuredPresentation = {
  id: 102,
  product_id: 11,
  name: "Kilogramo",
  mode: "measured" as const,
  base_quantity: 1000,
  price: 4000,
  barcode: null,
  is_default: false,
  is_active: false,
  is_conversion_locked: true,
  created_at: "2026-01-01",
  updated_at: "2026-01-01",
};

const view: ProductView = {
  stats: { product_count: 1, piece_stock: 0, gram_stock: 1250, milliliter_stock: 0, brand_count: 1, inventory_value: 0 },
  items: [{
    id: 11,
    name: "Cafe molido",
    sku: "CAFE-1",
    price: 1250,
    stock: 5,
    stock_base: 1250,
    base_unit: "gram",
    inventory_cost_cents: null,
    inventory_cost_status: "unknown",
    average_cost_cents: null,
    brand_id: 1,
    brand_name: "Origen",
    category_id: 2,
    category_name: "Bebidas",
    created_at: "2026-01-01",
    updated_at: "2026-01-01",
    presentations: [piecePresentation, measuredPresentation],
    default_presentation: piecePresentation,
  }],
};

beforeEach(() => {
  vi.useFakeTimers();
  mockedListBrands.mockResolvedValue([{ id: 1, name: "Origen", created_at: "", updated_at: "" }]);
  mockedListCategories.mockResolvedValue([{ id: 2, name: "Bebidas", created_at: "", updated_at: "" }]);
  mockedGetProductsView.mockResolvedValue(view);
  vi.mocked(createBrand).mockResolvedValue({ id: 1, name: "Origen", created_at: "", updated_at: "" });
  vi.mocked(updateBrand).mockResolvedValue({ id: 1, name: "Origen", created_at: "", updated_at: "" });
  vi.mocked(createCategory).mockResolvedValue({ id: 2, name: "Bebidas", created_at: "", updated_at: "" });
  vi.mocked(updateCategory).mockResolvedValue({ id: 2, name: "Bebidas", created_at: "", updated_at: "" });
});

async function renderLoaded() {
  render(<ProductsView sessionId={1} onNotice={vi.fn()} onSessionRequired={vi.fn()} />);
  await act(() => vi.advanceTimersByTimeAsync(250));
  await act(async () => undefined);
  expect(screen.getByText("Cafe molido")).toBeTruthy();
}

describe("ProductsView", () => {
  it("sends search, brand and category as remote filters", async () => {
    await renderLoaded();
    fireEvent.change(screen.getByLabelText("Buscar"), { target: { value: "cafe" } });
    fireEvent.change(screen.getByLabelText("Marca"), { target: { value: "1" } });
    fireEvent.change(screen.getByLabelText("Categoria"), { target: { value: "2" } });
    await act(() => vi.advanceTimersByTimeAsync(250));

    expect(mockedGetProductsView).toHaveBeenLastCalledWith({ search: "cafe", brand_id: 1, category_id: 2 });
  });

  it("names each product edit action with its product", async () => {
    mockedGetProductsView.mockResolvedValueOnce({
      ...view,
      stats: { ...view.stats, product_count: 2 },
      items: [view.items[0], { ...view.items[0], id: 12, name: "Te verde", sku: "TE-1" }],
    });
    await renderLoaded();

    expect(screen.getByRole("button", { name: "Editar producto Cafe molido" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Editar producto Te verde" })).toBeTruthy();
  });

  it("creates one SKU with presentations, initial cost and blocks duplicate submits", async () => {
    const request = deferred<Awaited<ReturnType<typeof createProductWithPresentations>>>();
    mockedCreateProduct.mockReturnValue(request.promise);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Nuevo producto" }));
    fireEvent.change(screen.getAllByLabelText(/^Nombre/)[0], { target: { value: "Te verde" } });
    fireEvent.change(screen.getByLabelText(/^SKU unico/), { target: { value: "TE-1" } });
    fireEvent.change(screen.getByLabelText(/^Unidad base de inventario/), { target: { value: "gram" } });
    expect((screen.getByLabelText(/^Nombre de presentacion/) as HTMLInputElement).value).toBe("Peso exacto");
    fireEvent.change(screen.getByLabelText(/^Nombre de presentacion/), { target: { value: "Kilogramo" } });
    fireEvent.change(screen.getByLabelText(/^Precio de venta/), { target: { value: "19.95" } });
    fireEvent.change(screen.getByLabelText(/^Codigo de barras/), { target: { value: "750000000099" } });
    fireEvent.click(screen.getByRole("button", { name: "Agregar presentacion" }));
    fireEvent.click(screen.getAllByRole("radio", { name: "Paquete" })[1]);
    expect((screen.getAllByLabelText(/^Nombre de presentacion/)[1] as HTMLInputElement).value).toBe("Paquete");
    fireEvent.change(screen.getAllByLabelText(/^Nombre de presentacion/)[1], { target: { value: "Bolsa 250 g" } });
    expect(screen.getByRole("button", { name: "Quitar presentación Kilogramo" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Quitar presentación Bolsa 250 g" })).toBeTruthy();
    expect(screen.queryByText(/Ejemplos: piezas, bolsas, cajas o botellas/)).toBeNull();
    fireEvent.change(screen.getByLabelText(/^Cantidad contenida en gramos/), { target: { value: "250" } });
    fireEvent.change(screen.getAllByLabelText(/^Precio de venta/)[1], { target: { value: "6.00" } });
    fireEvent.change(screen.getByLabelText(/^Stock inicial/), { target: { value: "3000" } });
    fireEvent.change(screen.getByLabelText(/^Precio de compra/), { target: { value: "0.02" } });
    expect(screen.getByText("$60.00")).toBeTruthy();
    expect(screen.getByText("El valor definitivo se confirma al guardar.")).toBeTruthy();
    expect(screen.getAllByLabelText(/SKU/)).toHaveLength(1);
    const form = document.getElementById("product-form")!;
    fireEvent.submit(form);
    fireEvent.submit(form);

    expect(mockedCreateProduct).toHaveBeenCalledOnce();
    expect(mockedCreateProduct).toHaveBeenCalledWith({
      name: "Te verde",
      sku: "TE-1",
      brand_id: 1,
      category_id: 2,
      base_unit: "gram",
      initial_stock: 3000,
      initial_purchase_price_per_base: "0.02",
      presentations: [
        {
          name: "Kilogramo",
          mode: "measured",
          base_quantity: 1000,
          price: "19.95",
          barcode: "750000000099",
          is_default: true,
          is_active: true,
        },
        {
          name: "Bolsa 250 g",
          mode: "fixed",
          base_quantity: 250,
          price: "6.00",
          barcode: null,
          is_default: false,
          is_active: true,
        },
      ],
    });
    request.resolve(view.items[0]);
    await act(async () => request.promise);
  });

  it("creates initial stock without valuation when purchase price is omitted", async () => {
    mockedCreateProduct.mockResolvedValueOnce(view.items[0]);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Nuevo producto" }));
    fireEvent.change(screen.getAllByLabelText(/^Nombre/)[0], { target: { value: "Producto sin costo" } });
    fireEvent.change(screen.getByLabelText(/^SKU unico/), { target: { value: "SIN-COSTO-1" } });
    fireEvent.change(screen.getByLabelText(/^Precio de venta/), { target: { value: "12.00" } });
    fireEvent.change(screen.getByLabelText(/^Stock inicial/), { target: { value: "5" } });

    await act(async () => {
      fireEvent.submit(document.getElementById("product-form")!);
    });

    expect(mockedCreateProduct).toHaveBeenCalledOnce();
    expect(mockedCreateProduct).toHaveBeenCalledWith(
      expect.objectContaining({
        initial_stock: 5,
        initial_purchase_price_per_base: null,
        presentations: [expect.objectContaining({ name: "Pieza", mode: "fixed", base_quantity: 1, is_default: true, is_active: true })],
      }),
    );
  });

  it("keeps the base unit readonly and reactivates a historical presentation as default", async () => {
    mockedUpdateProduct.mockResolvedValue(view.items[0]);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Editar producto Cafe molido" }));
    expect((screen.getByLabelText(/^Unidad base de inventario/) as HTMLSelectElement).disabled).toBe(true);
    expect((screen.getAllByRole("radio", { name: "Paquete" })[0].closest("fieldset") as HTMLFieldSetElement).disabled).toBe(true);
    expect((screen.getByLabelText(/^Cantidad contenida en gramos/) as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByLabelText(/^Base del precio en gramos/) as HTMLInputElement).disabled).toBe(true);
    expect(screen.getAllByText("Conversion bloqueada por historial.")).toHaveLength(2);
    expect(screen.getAllByText("Historica")).toHaveLength(2);
    expect((screen.getAllByLabelText(/^Nombre de presentacion/)[1] as HTMLInputElement).disabled).toBe(false);
    expect((screen.getAllByLabelText(/^Precio de venta/)[1] as HTMLInputElement).disabled).toBe(false);
    expect((screen.getAllByLabelText(/^Codigo de barras/)[1] as HTMLInputElement).disabled).toBe(false);
    expect((screen.getByRole("switch", { name: "Kilogramo activa" }) as HTMLInputElement).disabled).toBe(false);
    expect(screen.queryByLabelText(/^Stock inicial/)).toBeNull();
    fireEvent.click(screen.getByRole("switch", { name: "Kilogramo activa" }));
    fireEvent.click(screen.getByRole("button", { name: "Usar Kilogramo como predeterminada, presentación 2" }));
    fireEvent.click(screen.getByRole("switch", { name: "Bolsa 250 g activa" }));
    expect(screen.queryByRole("button", { name: /^Quitar presentación/ })).toBeNull();
    expect(screen.getAllByText("Predeterminada")).toHaveLength(2);
    expect(screen.getByText("Presentacion predeterminada")).toBeTruthy();
    expect(screen.getByText("Inactiva")).toBeTruthy();
    fireEvent.submit(document.getElementById("product-form")!);
    await act(async () => undefined);

    expect(mockedUpdateProduct).toHaveBeenCalledWith(11, {
      name: "Cafe molido",
      sku: "CAFE-1",
      brand_id: 1,
      category_id: 2,
      base_unit: "gram",
      presentations: [
        { id: 101, name: "Bolsa 250 g", mode: "fixed", base_quantity: 250, price: "12.50", barcode: "750000000001", is_default: false, is_active: false },
        { id: 102, name: "Kilogramo", mode: "measured", base_quantity: 1000, price: "40.00", barcode: null, is_default: true, is_active: true },
      ],
    });
  });

  it("allows removing a persisted presentation when its conversion is not locked", async () => {
    const unlockedPresentation = { ...measuredPresentation, is_conversion_locked: false };
    mockedGetProductsView.mockResolvedValueOnce({
      ...view,
      items: [{ ...view.items[0], presentations: [piecePresentation, unlockedPresentation] }],
    });
    mockedUpdateProduct.mockResolvedValue(view.items[0]);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Editar producto Cafe molido" }));

    expect(screen.getAllByText("Conversion bloqueada por historial.")).toHaveLength(1);
    expect(screen.getAllByText("Historica")).toHaveLength(1);
    expect((screen.getAllByRole("radio", { name: "Peso exacto" })[1] as HTMLInputElement).disabled).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Quitar presentación Kilogramo" }));
    expect(screen.queryByText("Kilogramo")).toBeNull();

    fireEvent.submit(document.getElementById("product-form")!);
    await act(async () => undefined);
    expect(mockedUpdateProduct).toHaveBeenCalledWith(11, expect.objectContaining({
      presentations: [expect.objectContaining({ id: 101 })],
    }));
  });

  it("shows formatted stock, active presentations and unknown cost separately", async () => {
    await renderLoaded();
    expect(screen.getAllByText("1.25 kg").length).toBeGreaterThan(0);
    expect(screen.getByText("Bolsa 250 g")).toBeTruthy();
    expect(screen.getByText("$12.50 por Bolsa 250 g")).toBeTruthy();
    expect(screen.getAllByText("Sin costo registrado").length).toBeGreaterThan(0);
    expect(screen.getByText("No incluidos en el valor a costo")).toBeTruthy();
  });

  it("ignores out-of-order list responses and completion after unmount", async () => {
    const first = deferred<ProductView>();
    const second = deferred<ProductView>();
    mockedGetProductsView.mockResolvedValueOnce(view).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { unmount } = render(<ProductsView sessionId={1} onNotice={vi.fn()} onSessionRequired={vi.fn()} />);
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.change(screen.getByLabelText("Buscar"), { target: { value: "a" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    fireEvent.change(screen.getByLabelText("Buscar"), { target: { value: "b" } });
    await act(() => vi.advanceTimersByTimeAsync(250));

    await act(async () => second.resolve({ ...view, items: [{ ...view.items[0], name: "Resultado B" }] }));
    expect(screen.getByText("Resultado B")).toBeTruthy();
    await act(async () => first.resolve({ ...view, items: [{ ...view.items[0], name: "Resultado A" }] }));
    expect(screen.queryByText("Resultado A")).toBeNull();

    const afterUnmount = deferred<ProductView>();
    mockedGetProductsView.mockReturnValueOnce(afterUnmount.promise);
    fireEvent.change(screen.getByLabelText("Buscar"), { target: { value: "c" } });
    await act(() => vi.advanceTimersByTimeAsync(250));
    unmount();
    await act(async () => afterUnmount.resolve(view));
  });

  it("marks previous results stale and hides query stats while new filters fail", async () => {
    await renderLoaded();
    const request = deferred<ProductView>();
    mockedGetProductsView.mockReturnValueOnce(request.promise);
    fireEvent.change(screen.getByLabelText("Buscar"), { target: { value: "nuevo" } });
    expect(screen.getByText("Actualizando; se muestran temporalmente los resultados anteriores.")).toBeTruthy();
    expect(screen.getAllByText("-")).toHaveLength(6);
    expect(screen.getByText("Cafe molido")).toBeTruthy();
    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => request.reject(new Error("SQLITE details")));
    expect(screen.getByText("Se muestran los resultados anteriores; no corresponden a los filtros actuales.")).toBeTruthy();
    expect(screen.queryByText("No hay productos para mostrar")).toBeNull();
    expect(screen.getAllByText("-")).toHaveLength(6);
  });

  it("starts piece products with an active default Pieza and offers only Pieza or Paquete", async () => {
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Nuevo producto" }));
    expect((screen.getByRole("radio", { name: "Pieza" }) as HTMLInputElement).checked).toBe(true);
    expect(screen.getByRole("radio", { name: "Paquete" })).toBeTruthy();
    expect(screen.queryByRole("radio", { name: /exacto/i })).toBeNull();
    expect((screen.getByLabelText(/^Nombre de presentacion/) as HTMLInputElement).value).toBe("Pieza");
    expect(screen.queryByLabelText(/^Cantidad contenida/)).toBeNull();
    expect((screen.getByRole("switch", { name: "Pieza activa" }) as HTMLInputElement).checked).toBe(true);
    expect(screen.getAllByText("Predeterminada")).toHaveLength(2);
    fireEvent.click(screen.getByRole("radio", { name: "Paquete" }));
    expect((screen.getByRole("radio", { name: "Paquete" }) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText(/^Nombre de presentacion/) as HTMLInputElement).value).toBe("Paquete");
    expect(screen.getByLabelText(/^Cantidad contenida en piezas/)).toBeTruthy();
    fireEvent.click(screen.getByRole("radio", { name: "Pieza" }));
    expect((screen.getByLabelText(/^Nombre de presentacion/) as HTMLInputElement).value).toBe("Pieza");
    expect(screen.queryByLabelText(/^Cantidad contenida/)).toBeNull();
  });

  it("supports arrow-key navigation in segmented sale-kind radios", async () => {
    await renderLoaded();
    vi.useRealTimers();
    const events = userEvent.setup();
    await events.click(screen.getByRole("button", { name: "Nuevo producto" }));
    await new Promise<void>((resolve) => window.requestAnimationFrame(() => resolve()));
    const piece = screen.getByRole("radio", { name: "Pieza" });
    const packageOption = screen.getByRole("radio", { name: "Paquete" });

    piece.focus();
    await events.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(packageOption);
    expect((packageOption as HTMLInputElement).checked).toBe(true);
    await events.keyboard("{ArrowLeft}");
    expect(document.activeElement).toBe(piece);
    expect((piece as HTMLInputElement).checked).toBe(true);
  });

  it("confirms before resetting captured presentations and preserves them on cancel", async () => {
    await renderLoaded();
    vi.useRealTimers();
    const events = userEvent.setup();
    await events.click(screen.getByRole("button", { name: "Nuevo producto" }));
    const unit = screen.getByLabelText(/^Unidad base de inventario/) as HTMLSelectElement;
    const name = screen.getByLabelText(/^Nombre de presentacion/) as HTMLInputElement;
    const price = screen.getByLabelText(/^Precio de venta/) as HTMLInputElement;
    await events.clear(name);
    await events.type(name, "Caja");
    await events.type(price, "25.00");

    await events.selectOptions(unit, "gram");
    expect(screen.getByRole("dialog", { name: "Cambiar unidad base" })).toBeTruthy();
    expect(unit.value).toBe("piece");
    await events.click(screen.getByRole("button", { name: "Conservar datos" }));
    expect(unit.value).toBe("piece");
    expect((screen.getByLabelText(/^Nombre de presentacion/) as HTMLInputElement).value).toBe("Caja");
    expect((screen.getByLabelText(/^Precio de venta/) as HTMLInputElement).value).toBe("25");

    await events.selectOptions(unit, "gram");
    await events.click(screen.getByRole("button", { name: "Cambiar y reiniciar" }));
    expect(unit.value).toBe("gram");
    expect((screen.getByLabelText(/^Nombre de presentacion/) as HTMLInputElement).value).toBe("Peso exacto");
    expect((screen.getByLabelText(/^Precio de venta/) as HTMLInputElement).value).toBe("");
  });

  it("changes an untouched base unit without confirmation", async () => {
    await renderLoaded();
    vi.useRealTimers();
    const events = userEvent.setup();
    await events.click(screen.getByRole("button", { name: "Nuevo producto" }));
    await events.selectOptions(screen.getByLabelText(/^Unidad base de inventario/), "gram");
    expect(screen.queryByRole("dialog", { name: "Cambiar unidad base" })).toBeNull();
    expect((screen.getByLabelText(/^Nombre de presentacion/) as HTMLInputElement).value).toBe("Peso exacto");
  });

  it("gives default actions contextual names and restores focus after removing cards", async () => {
    await renderLoaded();
    vi.useRealTimers();
    const events = userEvent.setup();
    await events.click(screen.getByRole("button", { name: "Nuevo producto" }));
    await events.click(screen.getByRole("button", { name: "Agregar presentacion" }));
    await events.click(screen.getByRole("button", { name: "Agregar presentacion" }));

    expect(screen.getByRole("button", { name: "Usar Presentacion 2 como predeterminada, presentación 2" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Usar Presentacion 3 como predeterminada, presentación 3" })).toBeTruthy();
    await events.click(screen.getByRole("button", { name: "Quitar presentación Presentacion 2" }));
    expect(document.activeElement).toBe(screen.getByRole("region", { name: "Presentacion 2: Presentacion 2" }));
    await events.click(screen.getByRole("button", { name: "Quitar presentación Presentacion 2" }));
    expect(document.activeElement).toBe(screen.getByRole("region", { name: "Presentacion 1: Pieza" }));
  });

  it("switches gram sale kinds, proposed names and dynamic quantity fields", async () => {
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Nuevo producto" }));
    fireEvent.change(screen.getByLabelText(/^Unidad base de inventario/), { target: { value: "gram" } });
    expect((screen.getByRole("radio", { name: "Peso exacto" }) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText(/^Nombre de presentacion/) as HTMLInputElement).value).toBe("Peso exacto");
    expect(screen.getByLabelText(/^Base del precio en gramos/)).toBeTruthy();
    fireEvent.click(screen.getByRole("radio", { name: "Paquete" }));
    expect((screen.getByLabelText(/^Nombre de presentacion/) as HTMLInputElement).value).toBe("Paquete");
    expect(screen.getByLabelText(/^Cantidad contenida en gramos/)).toBeTruthy();
  });

  it("offers Paquete and Volumen exacto for milliliters and preserves measured payload", async () => {
    mockedCreateProduct.mockResolvedValueOnce(view.items[0]);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Nuevo producto" }));
    fireEvent.change(screen.getAllByLabelText(/^Nombre/)[0], { target: { value: "Aceite" } });
    fireEvent.change(screen.getByLabelText(/^SKU unico/), { target: { value: "ACEITE-1" } });
    fireEvent.change(screen.getByLabelText(/^Unidad base de inventario/), { target: { value: "milliliter" } });

    expect(screen.getByRole("radio", { name: "Paquete" })).toBeTruthy();
    expect((screen.getByRole("radio", { name: "Volumen exacto" }) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText(/^Nombre de presentacion/) as HTMLInputElement).value).toBe("Volumen exacto");
    fireEvent.change(screen.getByLabelText(/^Base del precio en mililitros/), { target: { value: "500" } });
    fireEvent.change(screen.getByLabelText(/^Precio de venta/), { target: { value: "35.00" } });
    fireEvent.submit(document.getElementById("product-form")!);
    await act(async () => undefined);

    expect(mockedCreateProduct).toHaveBeenCalledWith(expect.objectContaining({
      base_unit: "milliliter",
      presentations: [expect.objectContaining({ name: "Volumen exacto", mode: "measured", base_quantity: 500 })],
    }));
  });

  it("calculates the estimated initial value with exact decimal integers", async () => {
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Nuevo producto" }));
    fireEvent.change(screen.getByLabelText(/^Stock inicial/), { target: { value: "10000000000001" } });
    fireEvent.change(screen.getByLabelText(/^Precio de compra/), { target: { value: "0.01" } });
    expect(screen.getByText("$100,000,000,000.01")).toBeTruthy();
  });

  it("drops save completion and session callbacks after a session change", async () => {
    const request = deferred<Awaited<ReturnType<typeof updateProductWithPresentations>>>();
    mockedUpdateProduct.mockReturnValue(request.promise);
    const onNotice = vi.fn();
    const onSessionRequired = vi.fn();
    const rendered = render(<ProductsView sessionId={1} onNotice={onNotice} onSessionRequired={onSessionRequired} />);
    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => undefined);
    fireEvent.click(screen.getByRole("button", { name: "Editar producto Cafe molido" }));
    fireEvent.submit(document.getElementById("product-form")!);
    rendered.rerender(<ProductsView sessionId={2} onNotice={onNotice} onSessionRequired={onSessionRequired} />);
    await act(async () => request.reject({ category: "authentication", code: "SESSION_REQUIRED", message: "Sesion vencida" }));
    expect(onNotice).not.toHaveBeenCalled();
    expect(onSessionRequired).not.toHaveBeenCalled();
  });

  it("drops catalog save completion after unmount", async () => {
    const request = deferred<Awaited<ReturnType<typeof createBrand>>>();
    vi.mocked(createBrand).mockReturnValue(request.promise);
    const onNotice = vi.fn();
    const onSessionRequired = vi.fn();
    const rendered = render(<ProductsView sessionId={1} onNotice={onNotice} onSessionRequired={onSessionRequired} />);
    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => undefined);
    fireEvent.click(screen.getByRole("button", { name: "Marcas y categorias" }));
    fireEvent.change(screen.getByLabelText(/^Nueva marcas/), { target: { value: "Nueva" } });
    fireEvent.click(screen.getAllByRole("button", { name: "Crear" })[0]);
    rendered.unmount();
    await act(async () => request.reject({ category: "authentication", code: "SESSION_REQUIRED", message: "Sesion vencida" }));
    expect(onNotice).not.toHaveBeenCalled();
    expect(onSessionRequired).not.toHaveBeenCalled();
  });
});
