import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import { createBrand, listBrands, updateBrand } from "../../api/brands";
import { createCategory, listCategories, updateCategory } from "../../api/categories";
import { createProductWithPresentations, getProductsView, updateProductWithPresentations } from "../../api/products";
import { Button, EmptyState, Field, Modal, Spinner, StatCard } from "../../components/ui";
import { errorMessage, isSessionRequired } from "../../lib/app-error";
import { centsToDecimal, formatBaseQuantity, formatMoney, presentationPriceLabel } from "../../lib/format";
import type { Brand } from "../../types/brand";
import type { Category } from "../../types/category";
import type { BaseUnit, ProductView, ProductViewRow } from "../../types/product";
import type { PresentationMode } from "../../types/product_presentation";

type Notice = { tone: "success" | "error" | "info"; message: string };
type ProductsViewProps = {
  sessionId: number;
  onNotice: (notice: Notice) => void;
  onSessionRequired: () => void;
};

type ProductDraft = {
  name: string;
  sku: string;
  baseUnit: BaseUnit;
  brandId: string;
  categoryId: string;
  presentations: PresentationDraft[];
};

type SaleKind = "piece" | "package" | "standard" | "custom" | "legacy";

type PresentationDraft = {
  key: number;
  id?: number;
  name: string;
  saleKind: SaleKind;
  mode: PresentationMode;
  baseQuantity: string;
  price: string;
  barcode: string;
  isActive: boolean;
  isDefault: boolean;
  isConversionLocked: boolean;
};

const unitLabels: Record<BaseUnit, { singular: string; plural: string }> = {
  piece: { singular: "pieza", plural: "piezas" },
  gram: { singular: "gramo", plural: "gramos" },
  milliliter: { singular: "mililitro", plural: "mililitros" },
  centimeter: { singular: "centimetro", plural: "centimetros" },
};

const measuredUnitConfig = {
  gram: { name: "Kilogramo", baseQuantity: 1_000 },
  milliliter: { name: "Litro", baseQuantity: 1_000 },
  centimeter: { name: "Metro", baseQuantity: 100 },
} satisfies Record<Exclude<BaseUnit, "piece">, { name: string; baseQuantity: number }>;

function standardBaseQuantity(baseUnit: BaseUnit): number | undefined {
  return baseUnit === "piece" ? undefined : measuredUnitConfig[baseUnit].baseQuantity;
}

function initialPresentation(baseUnit: BaseUnit, key = 0): PresentationDraft {
  const measured = baseUnit === "piece" ? undefined : measuredUnitConfig[baseUnit];
  return {
    key,
    name: measured?.name ?? "Pieza",
    saleKind: baseUnit === "piece" ? "piece" : "standard",
    mode: baseUnit === "piece" ? "fixed" : "measured",
    baseQuantity: String(measured?.baseQuantity ?? 1),
    price: "",
    barcode: "",
    isActive: true,
    isDefault: true,
    isConversionLocked: false,
  };
}

function hasMeaningfulPresentationData(draft: ProductDraft): boolean {
  if (draft.presentations.length !== 1) return true;
  const presentation = draft.presentations[0];
  const initial = initialPresentation(draft.baseUnit, presentation.key);
  return presentation.name !== initial.name
    || presentation.saleKind !== initial.saleKind
    || presentation.mode !== initial.mode
    || presentation.baseQuantity !== initial.baseQuantity
    || presentation.price.trim() !== ""
    || presentation.barcode.trim() !== ""
    || presentation.isActive !== initial.isActive
    || presentation.isDefault !== initial.isDefault;
}

function saleKindOptions(baseUnit: BaseUnit): { kind: SaleKind; label: string }[] {
  if (baseUnit === "piece") return [{ kind: "piece", label: "Pieza" }, { kind: "package", label: "Paquete" }];
  return [{ kind: "standard", label: measuredUnitConfig[baseUnit].name }, { kind: "custom", label: "Otra medida" }];
}

function saleKindChanges(kind: SaleKind, baseUnit: BaseUnit, currentQuantity: string): Partial<PresentationDraft> {
  if (kind === "piece") return { name: "Pieza", saleKind: kind, mode: "fixed", baseQuantity: "1" };
  if (kind === "package") return { name: "Paquete", saleKind: kind, mode: "fixed", baseQuantity: currentQuantity || "1" };
  const standard = baseUnit === "piece" ? undefined : measuredUnitConfig[baseUnit];
  if (kind === "standard" && standard) return { name: standard.name, saleKind: kind, mode: "measured", baseQuantity: String(standard.baseQuantity) };
  return { name: "", saleKind: kind, mode: "measured", baseQuantity: currentQuantity || "1" };
}

const emptyDraft: ProductDraft = {
  name: "",
  sku: "",
  baseUnit: "piece",
  brandId: "",
  categoryId: "",
  presentations: [initialPresentation("piece")],
};
const selectClass = "h-11 w-full rounded-control border border-line-strong bg-surface px-3 text-sm text-ink outline-none focus:border-brand focus:ring-3 focus:ring-brand/12 disabled:bg-surface-soft";
type LoadResult = "success" | "error" | "stale";

function SelectField({ label, value, onChange, children, required = false, disabled = false, hint }: { label: string; value: string; onChange: (value: string) => void; children: React.ReactNode; required?: boolean; disabled?: boolean; hint?: string }) {
  const id = useId();
  const hintId = hint ? `${id}-hint` : undefined;
  return <div className="grid gap-2"><label htmlFor={id} className="text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">{label}{required && <span className="sr-only"> obligatorio</span>}</label><select id={id} aria-describedby={hintId} className={selectClass} value={value} onChange={(event) => onChange(event.target.value)} required={required} disabled={disabled}>{children}</select>{hint && <p id={hintId} className="text-xs leading-5 text-muted">{hint}</p>}</div>;
}

export function ProductsView({ sessionId, onNotice, onSessionRequired }: ProductsViewProps) {
  const [view, setView] = useState<ProductView>();
  const [brands, setBrands] = useState<Brand[]>([]);
  const [categories, setCategories] = useState<Category[]>([]);
  const [search, setSearch] = useState("");
  const [brandFilter, setBrandFilter] = useState("");
  const [categoryFilter, setCategoryFilter] = useState("");
  const [loading, setLoading] = useState(true);
  const [tableLoading, setTableLoading] = useState(true);
  const [catalogError, setCatalogError] = useState("");
  const [viewError, setViewError] = useState("");
  const [productOpen, setProductOpen] = useState(false);
  const [editingProduct, setEditingProduct] = useState<ProductViewRow>();
  const [draft, setDraft] = useState<ProductDraft>(emptyDraft);
  const [savingProduct, setSavingProduct] = useState(false);
  const [productError, setProductError] = useState("");
  const [pendingBaseUnit, setPendingBaseUnit] = useState<BaseUnit>();
  const [catalogOpen, setCatalogOpen] = useState(false);
  const catalogRequest = useRef(0);
  const viewRequest = useRef(0);
  const productSaveInFlight = useRef(false);
  const productSaveGeneration = useRef(0);
  const presentationKey = useRef(1);
  const presentationRefs = useRef(new Map<number, HTMLElement>());
  const addPresentationRef = useRef<HTMLDivElement>(null);
  const pendingPresentationFocus = useRef<number | "add" | null>(null);
  const mounted = useRef(true);
  const lifetime = useRef(0);
  const sessionIdRef = useRef(sessionId);
  sessionIdRef.current = sessionId;

  function isCurrent(ownerSession: number, generation: number) {
    return mounted.current && sessionIdRef.current === ownerSession && lifetime.current === generation;
  }

  function fail(value: unknown, fallback: string, setLocalError?: (message: string) => void) {
    if (isSessionRequired(value)) {
      onSessionRequired();
      return;
    }
    const message = errorMessage(value, fallback);
    if (setLocalError) setLocalError(message);
    else setViewError(message);
  }

  async function loadCatalogs(): Promise<LoadResult> {
    const request = ++catalogRequest.current;
    const ownerSession = sessionId;
    const generation = lifetime.current;
    setLoading(true);
    try {
      const [nextBrands, nextCategories] = await Promise.all([listBrands(), listCategories()]);
      if (!isCurrent(ownerSession, generation) || catalogRequest.current !== request) return "stale";
      setBrands(nextBrands);
      setCategories(nextCategories);
      setCatalogError("");
      return "success";
    } catch (value: unknown) {
      if (!isCurrent(ownerSession, generation) || catalogRequest.current !== request) return "stale";
      if (isSessionRequired(value)) onSessionRequired();
      else setCatalogError(errorMessage(value, "No se pudieron cargar los catalogos."));
      return "error";
    } finally {
      if (isCurrent(ownerSession, generation) && catalogRequest.current === request) setLoading(false);
    }
  }

  async function loadView(): Promise<LoadResult> {
    const request = ++viewRequest.current;
    const ownerSession = sessionId;
    const generation = lifetime.current;
    setTableLoading(true);
    try {
      const nextView = await getProductsView({
        search: search || undefined,
        brand_id: brandFilter ? Number(brandFilter) : undefined,
        category_id: categoryFilter ? Number(categoryFilter) : undefined,
      });
      if (!isCurrent(ownerSession, generation) || viewRequest.current !== request) return "stale";
      setView(nextView);
      setViewError("");
      return "success";
    } catch (value: unknown) {
      if (!isCurrent(ownerSession, generation) || viewRequest.current !== request) return "stale";
      if (isSessionRequired(value)) onSessionRequired();
      else setViewError(errorMessage(value, "No se pudieron consultar los productos."));
      return "error";
    } finally {
      if (isCurrent(ownerSession, generation) && viewRequest.current === request) setTableLoading(false);
    }
  }

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      lifetime.current += 1;
      catalogRequest.current += 1;
      viewRequest.current += 1;
      productSaveGeneration.current += 1;
      productSaveInFlight.current = false;
    };
  }, []);

  useEffect(() => {
    lifetime.current += 1;
    catalogRequest.current += 1;
    viewRequest.current += 1;
    productSaveGeneration.current += 1;
    productSaveInFlight.current = false;
    setSavingProduct(false);
    setProductOpen(false);
    setPendingBaseUnit(undefined);
    setCatalogOpen(false);
    setView(undefined);
    void loadCatalogs();
  }, [sessionId]);

  useEffect(() => {
    const timeout = window.setTimeout(() => void loadView(), 250);
    return () => {
      window.clearTimeout(timeout);
      viewRequest.current += 1;
    };
  }, [sessionId, search, brandFilter, categoryFilter]);

  useEffect(() => {
    const target = pendingPresentationFocus.current;
    if (target === null) return;
    pendingPresentationFocus.current = null;
    if (target === "add") addPresentationRef.current?.querySelector("button")?.focus();
    else presentationRefs.current.get(target)?.focus();
  }, [draft.presentations]);

  function openCreate() {
    setEditingProduct(undefined);
    setPendingBaseUnit(undefined);
    presentationKey.current = 1;
    setDraft({
      ...emptyDraft,
      presentations: [initialPresentation("piece")],
      brandId: brands[0]?.id.toString() ?? "",
      categoryId: categories[0]?.id.toString() ?? "",
    });
    setProductError("");
    setProductOpen(true);
  }

  function openEdit(product: ProductViewRow) {
    setEditingProduct(product);
    setPendingBaseUnit(undefined);
    setDraft({
      name: product.name,
      sku: product.sku,
      baseUnit: product.base_unit,
      brandId: product.brand_id.toString(),
      categoryId: product.category_id.toString(),
      presentations: product.presentations.map((presentation, index) => ({
        key: index,
        id: presentation.id,
        name: presentation.name,
        saleKind: product.base_unit !== "piece" && presentation.mode === "fixed"
          ? "legacy"
          : presentation.mode === "measured"
          ? presentation.base_quantity === standardBaseQuantity(product.base_unit) ? "standard" : "custom"
          : product.base_unit === "piece" && presentation.base_quantity === 1 ? "piece" : "package",
        mode: presentation.mode,
        baseQuantity: presentation.base_quantity.toString(),
        price: centsToDecimal(presentation.price),
        barcode: presentation.barcode ?? "",
        isActive: presentation.is_active,
        isDefault: presentation.is_default,
        isConversionLocked: Boolean(presentation.is_conversion_locked),
      })),
    });
    presentationKey.current = product.presentations.length;
    setProductError("");
    setProductOpen(true);
  }

  function applyBaseUnit(baseUnit: BaseUnit) {
    setDraft({ ...draft, baseUnit, presentations: [initialPresentation(baseUnit)] });
    setPendingBaseUnit(undefined);
  }

  function changeBaseUnit(baseUnit: BaseUnit) {
    if (baseUnit === draft.baseUnit) return;
    if (hasMeaningfulPresentationData(draft)) setPendingBaseUnit(baseUnit);
    else applyBaseUnit(baseUnit);
  }

  function removePresentation(index: number) {
    const nextPresentations = draft.presentations.filter((_, itemIndex) => itemIndex !== index);
    pendingPresentationFocus.current = nextPresentations[index]?.key ?? nextPresentations[index - 1]?.key ?? "add";
    setDraft({ ...draft, presentations: nextPresentations });
  }

  async function saveProduct(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (productSaveInFlight.current) return;
    const activePresentations = draft.presentations.filter((presentation) => presentation.isActive);
    const defaultPresentations = draft.presentations.filter((presentation) => presentation.isDefault && presentation.isActive);
    if (!activePresentations.length || defaultPresentations.length !== 1) {
      setProductError("Debe haber al menos una presentacion activa y una sola predeterminada activa.");
      return;
    }
    const invalidBase = draft.presentations.some((presentation) => {
      const canonicalBase = presentation.saleKind === "standard"
        ? standardBaseQuantity(draft.baseUnit)
        : Number(presentation.baseQuantity);
      return canonicalBase === undefined
        || (presentation.saleKind !== "standard" && !/^\d+$/.test(presentation.baseQuantity))
        || !Number.isSafeInteger(canonicalBase)
        || canonicalBase <= 0;
    });
    if (invalidBase) {
      setProductError("La base de cada presentacion debe ser un entero positivo y seguro.");
      return;
    }
    productSaveInFlight.current = true;
    const saveGeneration = ++productSaveGeneration.current;
    const ownerSession = sessionId;
    const generation = lifetime.current;
    setSavingProduct(true);
    setProductError("");
    let saved = false;
    try {
      const common = {
        name: draft.name,
        sku: draft.sku,
        brand_id: Number(draft.brandId),
        category_id: Number(draft.categoryId),
        base_unit: draft.baseUnit,
      };
      const presentationPayload = draft.presentations.map((presentation) => ({
        id: presentation.id,
        name: presentation.name,
        mode: presentation.mode,
        base_quantity: presentation.saleKind === "standard"
          ? standardBaseQuantity(draft.baseUnit)!
          : Number(presentation.baseQuantity),
        price: presentation.price,
        barcode: presentation.barcode.trim() || null,
        is_default: presentation.isDefault,
        is_active: presentation.isActive,
      }));
      if (editingProduct) {
        await updateProductWithPresentations(editingProduct.id, {
          ...common,
          presentations: presentationPayload,
        });
      } else {
        await createProductWithPresentations({
          ...common,
          presentations: presentationPayload.map(({ id: _id, ...presentation }) => presentation),
        });
      }
      if (!isCurrent(ownerSession, generation) || productSaveGeneration.current !== saveGeneration) return;
      saved = true;
    } catch (value: unknown) {
      if (!isCurrent(ownerSession, generation) || productSaveGeneration.current !== saveGeneration) return;
      fail(value, "No se pudo guardar el producto.", setProductError);
    } finally {
      if (isCurrent(ownerSession, generation) && productSaveGeneration.current === saveGeneration) {
        productSaveInFlight.current = false;
        setSavingProduct(false);
      }
    }
    if (!saved) return;
    setProductOpen(false);
    const successMessage = editingProduct ? "Producto actualizado." : "Producto creado.";
    onNotice({ tone: "success", message: successMessage });
    if (await loadView() === "error" && isCurrent(ownerSession, generation) && productSaveGeneration.current === saveGeneration) {
      onNotice({ tone: "info", message: `${successMessage} No se pudo recargar la lista; usa Reintentar carga.` });
    }
  }

  async function catalogsChanged(message: string) {
    const ownerSession = sessionId;
    const generation = lifetime.current;
    const results = await Promise.all([loadCatalogs(), loadView()]);
    if (!isCurrent(ownerSession, generation)) return;
    if (results.includes("error")) onNotice({ tone: "info", message: `${message} No se pudieron recargar todos los datos; usa Reintentar carga.` });
    else onNotice({ tone: "success", message });
  }

  function refreshAll() {
    void Promise.all([loadCatalogs(), loadView()]);
  }

  function updateViewFilter(setter: (value: string) => void, value: string) {
    setTableLoading(true);
    setViewError("");
    setter(value);
  }

  const statsUnavailable = tableLoading || Boolean(viewError) || !view;
  const statsDetail = tableLoading
    ? "Pendiente de actualizacion"
    : viewError
      ? "No disponible por el error actual"
      : "Segun filtros actuales";
  const statValue = (value: number | undefined, money = false) =>
    statsUnavailable || value === undefined ? "-" : money ? formatMoney(value) : value;

  if (loading && !view) return <main id="main-content" tabIndex={-1} className="grid min-h-[calc(100dvh-88px)] place-items-center"><Spinner className="size-6 text-brand" /></main>;

  return (
    <main id="main-content" tabIndex={-1} className="p-4 outline-none sm:p-6 min-[900px]:p-8">
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-6">
        <StatCard label="Productos" value={statValue(view?.stats.product_count)} detail={statsDetail} />
        <StatCard label="Existencia en piezas" value={statsUnavailable ? "-" : formatBaseQuantity(view.stats.piece_stock, "piece")} detail={statsDetail} />
        <StatCard label="Existencia por peso" value={statsUnavailable ? "-" : formatBaseQuantity(view.stats.gram_stock, "gram")} detail={statsDetail} />
        <StatCard label="Existencia por volumen" value={statsUnavailable ? "-" : formatBaseQuantity(view.stats.milliliter_stock, "milliliter")} detail={statsDetail} />
        <StatCard label="Existencia por longitud" value={statsUnavailable ? "-" : formatBaseQuantity(view.stats.centimeter_stock, "centimeter")} detail={statsDetail} />
        <StatCard label="Valor a costo" value={statValue(view?.stats.inventory_value_cents, true)} detail={statsUnavailable ? statsDetail : view.stats.products_without_cost ? `${view.stats.products_without_cost} sin costo conocido` : "Todos los costos son conocidos"} />
      </div>

      <section className="mt-5 rounded-card border border-line bg-surface">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line p-4 sm:p-5">
           <div><h2 id="products-table-title" className="font-heading text-xl font-bold">Catalogo de productos</h2><p className="mt-1 text-xs text-muted">Existencias, presentaciones y valoracion del negocio.</p></div>
          <div className="flex flex-wrap gap-2"><Button onClick={() => setCatalogOpen(true)}>Marcas y categorias</Button><Button variant="primary" onClick={openCreate} disabled={!brands.length || !categories.length}>Nuevo producto</Button></div>
        </div>
        <div className="grid gap-3 border-b border-line bg-surface-soft/60 p-4 sm:grid-cols-3 sm:p-5">
          <Field label="Buscar" value={search} onChange={(event) => updateViewFilter(setSearch, event.target.value)} placeholder="Nombre o SKU" />
          <SelectField label="Marca" value={brandFilter} onChange={(value) => updateViewFilter(setBrandFilter, value)}><option value="">Todas las marcas</option>{brands.map((brand) => <option key={brand.id} value={brand.id}>{brand.name}</option>)}</SelectField>
          <SelectField label="Categoria" value={categoryFilter} onChange={(value) => updateViewFilter(setCategoryFilter, value)}><option value="">Todas las categorias</option>{categories.map((category) => <option key={category.id} value={category.id}>{category.name}</option>)}</SelectField>
        </div>
        {(catalogError || viewError) && <div className="m-4 flex flex-wrap items-center justify-between gap-3 rounded-control border border-danger/25 bg-danger-soft px-4 py-3 text-sm text-danger" role="alert"><span>{[catalogError, viewError].filter(Boolean).join(" ")}</span><Button size="sm" onClick={refreshAll} disabled={loading || tableLoading}>Reintentar carga</Button></div>}
        {viewError && view && <p className="mx-4 mb-3 text-xs font-semibold text-muted">Se muestran los resultados anteriores; no corresponden a los filtros actuales.</p>}
        {tableLoading && view && <p className="mx-4 mb-3 text-xs font-semibold text-muted">Actualizando; se muestran temporalmente los resultados anteriores.</p>}
        <div className="relative overflow-x-auto focus:outline-none focus:ring-3 focus:ring-brand/20" role="region" aria-busy={tableLoading} aria-label="Tabla de productos; desplaza horizontalmente para ver todas las columnas" tabIndex={0}>
          {tableLoading && <div className="absolute inset-x-0 top-0 h-0.5 overflow-hidden bg-brand-soft"><div className="h-full w-1/2 bg-brand" /></div>}
          {!view && tableLoading ? <div className="grid min-h-64 place-items-center"><Spinner className="size-6 text-brand" /></div> : view?.items.length ? (
            <table aria-labelledby="products-table-title" className="w-full min-w-[980px] text-left text-sm">
              <caption className="sr-only">Listado de productos del catalogo</caption>
              <thead><tr className="text-[0.625rem] tracking-wider text-muted uppercase"><th className="border-b border-line px-5 py-3">Producto</th><th className="border-b border-line px-5 py-3">Predeterminada</th><th className="border-b border-line px-5 py-3 text-right">Stock</th><th className="border-b border-line px-5 py-3 text-right">Activas</th><th className="border-b border-line px-5 py-3 text-right">Valor a costo</th><th className="border-b border-line px-5 py-3"><span className="sr-only">Acciones</span></th></tr></thead>
              <tbody>{view.items.map((product) => <tr key={product.id} className="hover:bg-surface-soft/60">
                <td className="border-b border-line px-5 py-4"><strong className="block">{product.name}</strong><span className="text-xs text-muted">{product.sku} · {product.brand_name} / {product.category_name}</span></td>
                <td className="border-b border-line px-5 py-4"><span className="block font-semibold">{product.default_presentation.name}</span><span className="text-xs text-muted">{presentationPriceLabel(product.default_presentation, product.base_unit)}</span></td>
                <td className="border-b border-line px-5 py-4 text-right"><span className={`rounded-full px-2.5 py-1 text-xs font-bold ${product.stock_base === 0 ? "bg-danger-soft text-danger" : "bg-brand-soft text-brand"}`}>{formatBaseQuantity(product.stock_base, product.base_unit)}</span></td>
                <td className="border-b border-line px-5 py-4 text-right font-semibold">{product.presentations.filter((presentation) => presentation.is_active).length}</td>
                <td className="border-b border-line px-5 py-4 text-right">{product.inventory_cost_status === "unknown" || product.inventory_cost_cents === null ? <span className="text-xs font-semibold text-muted">Costo desconocido</span> : <><span className="block font-semibold">{formatMoney(product.inventory_cost_cents)}</span><span className="text-xs text-muted">Promedio: {product.average_cost_cents === null ? "-" : formatMoney(product.average_cost_cents)}</span></>}</td>
                <td className="border-b border-line px-5 py-4 text-right"><Button size="sm" variant="ghost" aria-label={`Editar producto ${product.name}`} onClick={() => openEdit(product)}>Editar</Button></td>
              </tr>)}</tbody>
            </table>
          ) : !tableLoading && !viewError && <EmptyState title="No hay productos para mostrar" description="Cambia los filtros o crea el primer producto." action={<Button onClick={openCreate} disabled={!brands.length || !categories.length}>Nuevo producto</Button>} />}
        </div>
      </section>

      <Modal open={productOpen} onClose={() => setProductOpen(false)} closeDisabled={savingProduct} title={editingProduct ? "Editar producto" : "Nuevo producto"} description={editingProduct ? "La unidad base y las presentaciones historicas se conservan." : "El producto se crea con stock cero; registra entradas desde Inventario."} size="lg" footer={<><Button disabled={savingProduct} onClick={() => setProductOpen(false)}>Cancelar</Button><Button form="product-form" type="submit" variant="primary" loading={savingProduct}>Guardar producto</Button></>}>
        <form id="product-form" onSubmit={(event) => void saveProduct(event)} className="grid gap-4 sm:grid-cols-2">
          {productError && <div className="rounded-control border border-danger/25 bg-danger-soft px-4 py-3 text-sm text-danger sm:col-span-2" role="alert">{productError}</div>}
          <Field data-autofocus label="Nombre" value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} required disabled={savingProduct} />
          <Field label="SKU unico del producto" value={draft.sku} onChange={(event) => setDraft({ ...draft, sku: event.target.value })} required disabled={savingProduct} hint="Las presentaciones no usan SKU." />
          <SelectField label="Unidad base de inventario" value={draft.baseUnit} onChange={(baseUnit) => changeBaseUnit(baseUnit as BaseUnit)} required disabled={savingProduct || Boolean(editingProduct)} hint={editingProduct ? "No se puede cambiar porque las existencias ya estan registradas." : "No se puede cambiar despues de crear el producto."}><option value="piece">Pieza</option><option value="gram">Gramo</option><option value="milliliter">Mililitro</option><option value="centimeter">Centimetro</option></SelectField>
          <SelectField label="Marca" value={draft.brandId} onChange={(brandId) => setDraft({ ...draft, brandId })} required disabled={savingProduct}><option value="">Selecciona una marca</option>{brands.map((brand) => <option key={brand.id} value={brand.id}>{brand.name}</option>)}</SelectField>
          <SelectField label="Categoria" value={draft.categoryId} onChange={(categoryId) => setDraft({ ...draft, categoryId })} required disabled={savingProduct}><option value="">Selecciona una categoria</option>{categories.map((category) => <option key={category.id} value={category.id}>{category.name}</option>)}</SelectField>
          <fieldset className="grid gap-3 sm:col-span-2" disabled={savingProduct}>
            <legend className="sr-only">Presentaciones</legend>
            <div className="flex flex-wrap items-center justify-between gap-2"><div><h2 className="font-heading text-lg font-bold">Presentaciones</h2><p className="mt-1 text-xs text-muted">Todas usan el SKU del producto; el codigo de barras es opcional.</p></div><div ref={addPresentationRef}><Button size="sm" onClick={() => setDraft({ ...draft, presentations: [...draft.presentations, { ...initialPresentation(draft.baseUnit, presentationKey.current++), name: "", isDefault: false }] })}>Agregar presentacion</Button></div></div>
            {draft.presentations.map((presentation, index) => {
              const kind = presentation.saleKind;
              const quantityLabel = kind === "legacy"
                ? `Cantidad fija por venta en ${unitLabels[draft.baseUnit].plural}`
                : kind === "package"
                  ? `Cantidad contenida en ${unitLabels[draft.baseUnit].plural}`
                  : `Base del precio en ${unitLabels[draft.baseUnit].plural}`;
              const summary = kind === "piece"
                ? "1 pieza por venta"
                : kind === "legacy"
                  ? `${presentation.baseQuantity || "-"} ${unitLabels[draft.baseUnit].plural} por venta fija historica`
                  : kind === "package"
                    ? `${presentation.baseQuantity || "-"} ${unitLabels[draft.baseUnit].plural} por paquete`
                  : `Precio por ${kind === "standard" ? standardBaseQuantity(draft.baseUnit) : presentation.baseQuantity || "-"} ${unitLabels[draft.baseUnit].plural}`;
              const updatePresentation = (changes: Partial<PresentationDraft>) => setDraft({ ...draft, presentations: draft.presentations.map((item) => item.key === presentation.key ? { ...item, ...changes } : item) });
              const title = presentation.name || `Presentacion ${index + 1}`;
              return <section key={presentation.key} ref={(node) => { if (node) presentationRefs.current.set(presentation.key, node); else presentationRefs.current.delete(presentation.key); }} tabIndex={-1} className="grid gap-3 rounded-card border border-line bg-surface p-4 shadow-sm outline-none focus-visible:ring-3 focus-visible:ring-brand/20 sm:grid-cols-2 lg:grid-cols-3" aria-label={`Presentacion ${index + 1}: ${title}`}>
                <div className="flex flex-wrap items-start justify-between gap-3 sm:col-span-2 lg:col-span-3">
                  <div><div className="flex flex-wrap items-center gap-2"><h3 className="font-bold">{title}</h3>{presentation.isDefault && <span className="rounded-full bg-brand-soft px-2 py-0.5 text-[0.625rem] font-extrabold tracking-wide text-brand uppercase">Predeterminada</span>}{!presentation.isActive && <span className="rounded-full bg-surface-soft px-2 py-0.5 text-[0.625rem] font-extrabold tracking-wide text-muted uppercase">Inactiva</span>}{presentation.isConversionLocked && <span className="rounded-full bg-surface-soft px-2 py-0.5 text-[0.625rem] font-extrabold tracking-wide text-muted uppercase">Historica</span>}</div><p className="mt-1 text-xs text-muted">{summary}</p></div>
                  {!presentation.isConversionLocked && draft.presentations.length > 1 && <Button size="sm" variant="ghost" aria-label={`Quitar presentación ${title}`} onClick={() => removePresentation(index)}>Quitar</Button>}
                </div>
                <Field label="Nombre de presentacion" value={presentation.name} onChange={(event) => updatePresentation({ name: event.target.value })} required />
                <fieldset className="grid gap-2" disabled={presentation.isConversionLocked || kind === "legacy"}>
                  <legend className="text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">Forma de venta</legend>
                  {kind === "legacy" ? <div className="rounded-control border border-line-strong bg-surface-soft p-1"><label className="block rounded-[calc(var(--radius-control)-0.25rem)] bg-surface px-3 py-2 text-center text-sm font-bold text-muted shadow-sm"><input className="sr-only" type="radio" name={`sale-kind-${presentation.key}`} value="legacy" checked readOnly disabled />Venta fija historica</label></div> : <div className="grid grid-cols-2 rounded-control border border-line-strong bg-surface-soft p-1">{saleKindOptions(draft.baseUnit).map((option) => <label key={option.kind} className={`cursor-pointer rounded-[calc(var(--radius-control)-0.25rem)] px-3 py-2 text-center text-sm font-bold outline-none transition has-focus-visible:ring-3 has-focus-visible:ring-brand/20 ${kind === option.kind ? "bg-surface text-brand shadow-sm" : "text-muted hover:text-ink"}`}><input className="sr-only" type="radio" name={`sale-kind-${presentation.key}`} value={option.kind} checked={kind === option.kind} onChange={() => updatePresentation(saleKindChanges(option.kind, draft.baseUnit, presentation.baseQuantity))} />{option.label}</label>)}</div>}
                </fieldset>
                {kind !== "piece" && <Field label={quantityLabel} type="number" min="1" max={Number.MAX_SAFE_INTEGER} step="1" value={kind === "standard" ? String(standardBaseQuantity(draft.baseUnit)) : presentation.baseQuantity} onChange={(event) => updatePresentation({ baseQuantity: event.target.value })} required disabled={presentation.isConversionLocked || kind === "standard" || kind === "legacy"} />}
                {presentation.isConversionLocked && <p className="text-xs font-semibold text-muted sm:col-span-2 lg:col-span-3">Conversion bloqueada por historial.</p>}
                <Field label="Precio de venta" type="number" min="0.01" step="0.01" value={presentation.price} onChange={(event) => updatePresentation({ price: event.target.value })} inputMode="decimal" placeholder="0.00" required />
                <Field label="Codigo de barras (opcional)" value={presentation.barcode} onChange={(event) => updatePresentation({ barcode: event.target.value })} />
                <div className="flex flex-wrap items-center justify-between gap-3 border-t border-line pt-3 sm:col-span-2 lg:col-span-3">
                  <label className={`flex items-center gap-2 text-sm font-semibold ${presentation.isDefault ? "cursor-not-allowed text-muted" : "cursor-pointer"}`}><input className="peer sr-only" type="checkbox" role="switch" aria-label={`${title} activa`} checked={presentation.isActive} disabled={presentation.isDefault} onChange={(event) => updatePresentation({ isActive: event.target.checked })} /><span aria-hidden="true" className="relative h-6 w-11 rounded-full bg-line-strong transition peer-checked:bg-brand peer-focus-visible:ring-3 peer-focus-visible:ring-brand/20 peer-disabled:opacity-50 after:absolute after:top-0.5 after:left-0.5 after:size-5 after:rounded-full after:bg-white after:shadow-sm after:transition-transform peer-checked:after:translate-x-5" />Activa</label>
                  {presentation.isDefault ? <span className="text-xs font-bold text-brand">Presentacion predeterminada</span> : <Button size="sm" variant="ghost" aria-label={`Usar ${title} como predeterminada, presentación ${index + 1}`} onClick={() => setDraft({ ...draft, presentations: draft.presentations.map((item) => ({ ...item, isDefault: item.key === presentation.key, isActive: item.key === presentation.key ? true : item.isActive })) })}>Usar como predeterminada</Button>}
                </div>
              </section>;
            })}
          </fieldset>
        </form>
      </Modal>

      <Modal open={pendingBaseUnit !== undefined} onClose={() => setPendingBaseUnit(undefined)} title="Cambiar unidad base" description="Este cambio reiniciara las presentaciones capturadas." size="sm" footer={<><Button onClick={() => setPendingBaseUnit(undefined)}>Conservar datos</Button><Button variant="danger" onClick={() => pendingBaseUnit && applyBaseUnit(pendingBaseUnit)}>Cambiar y reiniciar</Button></>}>
        <p className="text-sm text-muted">Se perderan los nombres, conversiones, precios y codigos de barras de las presentaciones actuales.</p>
      </Modal>

      <CatalogModal open={catalogOpen} sessionId={sessionId} brands={brands} categories={categories} onClose={() => setCatalogOpen(false)} onSessionRequired={onSessionRequired} onChanged={catalogsChanged} />
    </main>
  );
}

function CatalogModal({ open, sessionId, brands, categories, onClose, onChanged, onSessionRequired }: { open: boolean; sessionId: number; brands: Brand[]; categories: Category[]; onClose: () => void; onChanged: (message: string) => Promise<void>; onSessionRequired: () => void }) {
  const [brandId, setBrandId] = useState<number>();
  const [brandName, setBrandName] = useState("");
  const [categoryId, setCategoryId] = useState<number>();
  const [categoryName, setCategoryName] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const saveInFlight = useRef(false);
  const mounted = useRef(true);
  const generation = useRef(0);
  const sessionIdRef = useRef(sessionId);
  sessionIdRef.current = sessionId;

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; generation.current += 1; saveInFlight.current = false; };
  }, []);

  useEffect(() => {
    generation.current += 1;
    saveInFlight.current = false;
    setSaving(false);
    setError("");
  }, [open, sessionId]);

  async function save(kind: "brand" | "category", event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saveInFlight.current) return;
    saveInFlight.current = true;
    const saveGeneration = generation.current;
    const ownerSession = sessionId;
    setSaving(true);
    setError("");
    let saved = false;
    try {
      if (kind === "brand") {
        if (brandId) await updateBrand(brandId, { name: brandName });
        else await createBrand({ name: brandName });
      } else {
        if (categoryId) await updateCategory(categoryId, { name: categoryName });
        else await createCategory({ name: categoryName });
      }
      if (!mounted.current || generation.current !== saveGeneration || sessionIdRef.current !== ownerSession) return;
      saved = true;
    } catch (value: unknown) {
      if (!mounted.current || generation.current !== saveGeneration || sessionIdRef.current !== ownerSession) return;
      if (isSessionRequired(value)) onSessionRequired();
      else setError(errorMessage(value, "No se pudo guardar el catalogo."));
    } finally {
      if (mounted.current && generation.current === saveGeneration && sessionIdRef.current === ownerSession) {
        saveInFlight.current = false;
        setSaving(false);
      }
    }
    if (!saved) return;
    if (kind === "brand") {
      setBrandId(undefined);
      setBrandName("");
      await onChanged("Marca guardada.");
    } else {
      setCategoryId(undefined);
      setCategoryName("");
      await onChanged("Categoria guardada.");
    }
  }

  return <Modal open={open} onClose={onClose} closeDisabled={saving} title="Marcas y categorias" description="Crea o selecciona un registro para editarlo." size="md" footer={<Button onClick={onClose} disabled={saving}>Cerrar</Button>}>
    {error && <div className="mb-4 rounded-control border border-danger/25 bg-danger-soft px-4 py-3 text-sm text-danger" role="alert">{error}</div>}
    <div className="grid gap-6 md:grid-cols-2">
      <CatalogColumn title="Marcas" items={brands} selectedId={brandId} onSelect={(item) => { setBrandId(item.id); setBrandName(item.name); setError(""); }} onNew={() => { setBrandId(undefined); setBrandName(""); }} name={brandName} setName={setBrandName} saving={saving} onSubmit={(event) => void save("brand", event)} />
      <CatalogColumn title="Categorias" items={categories} selectedId={categoryId} onSelect={(item) => { setCategoryId(item.id); setCategoryName(item.name); setError(""); }} onNew={() => { setCategoryId(undefined); setCategoryName(""); }} name={categoryName} setName={setCategoryName} saving={saving} onSubmit={(event) => void save("category", event)} />
    </div>
  </Modal>;
}

function CatalogColumn<T extends { id: number; name: string }>({ title, items, selectedId, onSelect, onNew, name, setName, saving, onSubmit }: { title: string; items: T[]; selectedId?: number; onSelect: (item: T) => void; onNew: () => void; name: string; setName: (name: string) => void; saving: boolean; onSubmit: (event: FormEvent<HTMLFormElement>) => void }) {
  return <section><div className="flex items-center justify-between"><h3 className="font-heading text-lg font-bold">{title}</h3><Button size="sm" variant="ghost" onClick={onNew} disabled={saving}>Nuevo</Button></div><div className="mt-3 max-h-48 overflow-y-auto rounded-control border border-line">{items.map((item) => <button key={item.id} type="button" aria-pressed={selectedId === item.id} disabled={saving} onClick={() => onSelect(item)} className={`block w-full border-b border-line px-3 py-2 text-left text-sm last:border-0 disabled:cursor-not-allowed disabled:opacity-45 ${selectedId === item.id ? "bg-brand-soft font-bold text-brand" : "hover:bg-surface-soft"}`}>{item.name}</button>)}{!items.length && <p className="p-3 text-xs text-muted">Sin registros.</p>}</div><form className="mt-3 grid gap-3" onSubmit={onSubmit}><Field label={selectedId ? `Editar ${title.toLowerCase()}` : `Nueva ${title.toLowerCase()}`} value={name} onChange={(event) => setName(event.target.value)} required disabled={saving} /><Button type="submit" variant="primary" loading={saving}>{selectedId ? "Actualizar" : "Crear"}</Button></form></section>;
}
