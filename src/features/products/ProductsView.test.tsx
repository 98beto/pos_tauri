import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createBrand, listBrands, updateBrand } from "../../api/brands";
import { createCategory, listCategories, updateCategory } from "../../api/categories";
import { createProduct, getProductsView, updateProduct } from "../../api/products";
import { deferred } from "../../test/fixtures";
import type { ProductView } from "../../types/product";
import { ProductsView } from "./ProductsView";

vi.mock("../../api/brands", () => ({ createBrand: vi.fn(), listBrands: vi.fn(), updateBrand: vi.fn() }));
vi.mock("../../api/categories", () => ({ createCategory: vi.fn(), listCategories: vi.fn(), updateCategory: vi.fn() }));
vi.mock("../../api/products", () => ({ createProduct: vi.fn(), getProductsView: vi.fn(), updateProduct: vi.fn() }));

const mockedListBrands = vi.mocked(listBrands);
const mockedListCategories = vi.mocked(listCategories);
const mockedGetProductsView = vi.mocked(getProductsView);
const mockedCreateProduct = vi.mocked(createProduct);
const mockedUpdateProduct = vi.mocked(updateProduct);

const view: ProductView = {
  stats: { product_count: 1, inventory_units: 5, brand_count: 1, inventory_value: 6250 },
  items: [{
    id: 11,
    name: "Cafe molido",
    sku: "CAFE-1",
    price: 1250,
    stock: 5,
    brand_id: 1,
    brand_name: "Origen",
    category_id: 2,
    category_name: "Bebidas",
    created_at: "2026-01-01",
    updated_at: "2026-01-01",
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
  render(<ProductsView onNotice={vi.fn()} onSessionRequired={vi.fn()} />);
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

  it("creates with decimal price text and blocks duplicate submits", async () => {
    const request = deferred<Awaited<ReturnType<typeof createProduct>>>();
    mockedCreateProduct.mockReturnValue(request.promise);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Nuevo producto" }));
    fireEvent.change(screen.getByLabelText(/^Nombre/), { target: { value: "Te verde" } });
    fireEvent.change(screen.getByLabelText(/^SKU/), { target: { value: "TE-1" } });
    fireEvent.change(screen.getByLabelText(/^Precio/), { target: { value: "19.95" } });
    fireEvent.change(screen.getByLabelText(/^Stock inicial/), { target: { value: "3" } });
    const form = document.getElementById("product-form")!;
    fireEvent.submit(form);
    fireEvent.submit(form);

    expect(mockedCreateProduct).toHaveBeenCalledOnce();
    expect(mockedCreateProduct).toHaveBeenCalledWith({ name: "Te verde", sku: "TE-1", price: "19.95", stock: 3, brand_id: 1, category_id: 2 });
    request.resolve(view.items[0]);
    await act(async () => request.promise);
  });

  it("edits without stock and preserves price as decimal text", async () => {
    mockedUpdateProduct.mockResolvedValue(view.items[0]);
    await renderLoaded();
    fireEvent.click(screen.getByRole("button", { name: "Editar" }));
    expect((screen.getByLabelText(/^Precio/) as HTMLInputElement).value).toBe("12.50");
    fireEvent.change(screen.getByLabelText(/^Precio/), { target: { value: "13.40" } });
    fireEvent.submit(document.getElementById("product-form")!);
    await act(async () => undefined);

    expect(mockedUpdateProduct).toHaveBeenCalledWith(11, {
      name: "Cafe molido",
      sku: "CAFE-1",
      price: "13.40",
      brand_id: 1,
      category_id: 2,
    });
  });

  it("ignores out-of-order list responses and completion after unmount", async () => {
    const first = deferred<ProductView>();
    const second = deferred<ProductView>();
    mockedGetProductsView.mockResolvedValueOnce(view).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { unmount } = render(<ProductsView onNotice={vi.fn()} onSessionRequired={vi.fn()} />);
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
    expect(screen.getAllByText("-")).toHaveLength(4);
    expect(screen.getByText("Cafe molido")).toBeTruthy();
    await act(() => vi.advanceTimersByTimeAsync(250));
    await act(async () => request.reject(new Error("SQLITE details")));
    expect(screen.getByText("Se muestran los resultados anteriores; no corresponden a los filtros actuales.")).toBeTruthy();
    expect(screen.queryByText("No hay productos para mostrar")).toBeNull();
    expect(screen.getAllByText("-")).toHaveLength(4);
  });
});
