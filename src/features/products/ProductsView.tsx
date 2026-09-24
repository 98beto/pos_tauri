import { useEffect, useRef, useState, type FormEvent } from "react";
import { createBrand, listBrands, updateBrand } from "../../api/brands";
import { createCategory, listCategories, updateCategory } from "../../api/categories";
import { createProduct, getProductsView, updateProduct } from "../../api/products";
import { Button, EmptyState, Field, Modal, Spinner, StatCard } from "../../components/ui";
import { errorMessage, isSessionRequired } from "../../lib/app-error";
import { centsToDecimal, formatMoney } from "../../lib/format";
import type { Brand } from "../../types/brand";
import type { Category } from "../../types/category";
import type { ProductView, ProductViewRow } from "../../types/product";

type Notice = { tone: "success" | "error" | "info"; message: string };
type ProductsViewProps = {
  onNotice: (notice: Notice) => void;
  onSessionRequired: () => void;
};

type ProductDraft = {
  name: string;
  sku: string;
  price: string;
  stock: string;
  brandId: string;
  categoryId: string;
};

const emptyDraft: ProductDraft = { name: "", sku: "", price: "", stock: "", brandId: "", categoryId: "" };
const selectClass = "h-11 w-full rounded-control border border-line-strong bg-surface px-3 text-sm text-ink outline-none focus:border-brand focus:ring-3 focus:ring-brand/12 disabled:bg-surface-soft";
type LoadResult = "success" | "error" | "stale";

function SelectField({ label, value, onChange, children, required = false, disabled = false }: { label: string; value: string; onChange: (value: string) => void; children: React.ReactNode; required?: boolean; disabled?: boolean }) {
  return <label className="grid gap-2 text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">{label}{required && <span className="sr-only">obligatorio</span>}<select className={selectClass} value={value} onChange={(event) => onChange(event.target.value)} required={required} disabled={disabled}>{children}</select></label>;
}

export function ProductsView({ onNotice, onSessionRequired }: ProductsViewProps) {
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
  const [catalogOpen, setCatalogOpen] = useState(false);
  const catalogRequest = useRef(0);
  const viewRequest = useRef(0);
  const productSaveInFlight = useRef(false);

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
    setLoading(true);
    try {
      const [nextBrands, nextCategories] = await Promise.all([listBrands(), listCategories()]);
      if (catalogRequest.current !== request) return "stale";
      setBrands(nextBrands);
      setCategories(nextCategories);
      setCatalogError("");
      return "success";
    } catch (value: unknown) {
      if (catalogRequest.current !== request) return "stale";
      if (isSessionRequired(value)) onSessionRequired();
      else setCatalogError(errorMessage(value, "No se pudieron cargar los catalogos."));
      return "error";
    } finally {
      if (catalogRequest.current === request) setLoading(false);
    }
  }

  async function loadView(): Promise<LoadResult> {
    const request = ++viewRequest.current;
    setTableLoading(true);
    try {
      const nextView = await getProductsView({
        search: search || undefined,
        brand_id: brandFilter ? Number(brandFilter) : undefined,
        category_id: categoryFilter ? Number(categoryFilter) : undefined,
      });
      if (viewRequest.current !== request) return "stale";
      setView(nextView);
      setViewError("");
      return "success";
    } catch (value: unknown) {
      if (viewRequest.current !== request) return "stale";
      if (isSessionRequired(value)) onSessionRequired();
      else setViewError(errorMessage(value, "No se pudieron consultar los productos."));
      return "error";
    } finally {
      if (viewRequest.current === request) setTableLoading(false);
    }
  }

  useEffect(() => {
    void loadCatalogs();
    return () => { catalogRequest.current += 1; };
  }, []);

  useEffect(() => {
    const timeout = window.setTimeout(() => void loadView(), 250);
    return () => {
      window.clearTimeout(timeout);
      viewRequest.current += 1;
    };
  }, [search, brandFilter, categoryFilter]);

  function openCreate() {
    setEditingProduct(undefined);
    setDraft({ ...emptyDraft, brandId: brands[0]?.id.toString() ?? "", categoryId: categories[0]?.id.toString() ?? "" });
    setProductError("");
    setProductOpen(true);
  }

  function openEdit(product: ProductViewRow) {
    setEditingProduct(product);
    setDraft({
      name: product.name,
      sku: product.sku,
      price: centsToDecimal(product.price),
      stock: "",
      brandId: product.brand_id.toString(),
      categoryId: product.category_id.toString(),
    });
    setProductError("");
    setProductOpen(true);
  }

  async function saveProduct(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (productSaveInFlight.current) return;
    productSaveInFlight.current = true;
    setSavingProduct(true);
    setProductError("");
    let saved = false;
    try {
      const common = {
        name: draft.name,
        sku: draft.sku,
        price: draft.price,
        brand_id: Number(draft.brandId),
        category_id: Number(draft.categoryId),
      };
      if (editingProduct) await updateProduct(editingProduct.id, common);
      else await createProduct({ ...common, stock: Number(draft.stock) });
      saved = true;
    } catch (value: unknown) {
      fail(value, "No se pudo guardar el producto.", setProductError);
    } finally {
      productSaveInFlight.current = false;
      setSavingProduct(false);
    }
    if (!saved) return;
    setProductOpen(false);
    const successMessage = editingProduct ? "Producto actualizado." : "Producto creado.";
    onNotice({ tone: "success", message: successMessage });
    if (await loadView() === "error") {
      onNotice({ tone: "info", message: `${successMessage} No se pudo recargar la lista; usa Reintentar carga.` });
    }
  }

  async function catalogsChanged(message: string) {
    const results = await Promise.all([loadCatalogs(), loadView()]);
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
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <StatCard label="Productos" value={statValue(view?.stats.product_count)} detail={statsDetail} />
        <StatCard label="Unidades" value={statValue(view?.stats.inventory_units)} detail={statsDetail} />
        <StatCard label="Marcas" value={statValue(view?.stats.brand_count)} detail={statsDetail} />
        <StatCard label="Valor inventario" value={statValue(view?.stats.inventory_value, true)} detail={statsDetail} />
      </div>

      <section className="mt-5 rounded-card border border-line bg-surface">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line p-4 sm:p-5">
           <div><h2 id="products-table-title" className="font-heading text-xl font-bold">Catalogo de productos</h2><p className="mt-1 text-xs text-muted">Precios y clasificaciones del negocio.</p></div>
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
        <div className="relative overflow-x-auto" aria-busy={tableLoading}>{tableLoading && <div className="absolute inset-x-0 top-0 h-0.5 overflow-hidden bg-brand-soft"><div className="h-full w-1/2 bg-brand" /></div>}{!view && tableLoading ? <div className="grid min-h-64 place-items-center"><Spinner className="size-6 text-brand" /></div> : view?.items.length ? <table aria-labelledby="products-table-title" className="w-full min-w-[760px] text-left text-sm"><caption className="sr-only">Listado de productos del catalogo</caption><thead><tr className="text-[0.625rem] tracking-wider text-muted uppercase"><th className="border-b border-line px-5 py-3">Producto</th><th className="border-b border-line px-5 py-3">Marca / categoria</th><th className="border-b border-line px-5 py-3 text-right">Precio</th><th className="border-b border-line px-5 py-3 text-right">Stock</th><th className="border-b border-line px-5 py-3"><span className="sr-only">Acciones</span></th></tr></thead><tbody>{view.items.map((product) => <tr key={product.id} className="hover:bg-surface-soft/60"><td className="border-b border-line px-5 py-4"><strong className="block">{product.name}</strong><span className="text-xs text-muted">{product.sku}</span></td><td className="border-b border-line px-5 py-4"><span className="block">{product.brand_name}</span><span className="text-xs text-muted">{product.category_name}</span></td><td className="border-b border-line px-5 py-4 text-right font-semibold">{formatMoney(product.price)}</td><td className="border-b border-line px-5 py-4 text-right"><span className={`rounded-full px-2.5 py-1 text-xs font-bold ${product.stock === 0 ? "bg-danger-soft text-danger" : "bg-brand-soft text-brand"}`}>{product.stock}</span></td><td className="border-b border-line px-5 py-4 text-right"><Button size="sm" variant="ghost" onClick={() => openEdit(product)}>Editar</Button></td></tr>)}</tbody></table> : !tableLoading && !viewError && <EmptyState title="No hay productos para mostrar" description="Cambia los filtros o crea el primer producto." action={<Button onClick={openCreate} disabled={!brands.length || !categories.length}>Nuevo producto</Button>} />}</div>
      </section>

      <Modal open={productOpen} onClose={() => setProductOpen(false)} closeDisabled={savingProduct} title={editingProduct ? "Editar producto" : "Nuevo producto"} description={editingProduct ? "La edicion no modifica existencias." : "El stock inicial genera un movimiento de inventario."} size="md" footer={<><Button disabled={savingProduct} onClick={() => setProductOpen(false)}>Cancelar</Button><Button form="product-form" type="submit" variant="primary" loading={savingProduct}>Guardar producto</Button></>}>
        <form id="product-form" onSubmit={(event) => void saveProduct(event)} className="grid gap-4 sm:grid-cols-2">
          {productError && <div className="rounded-control border border-danger/25 bg-danger-soft px-4 py-3 text-sm text-danger sm:col-span-2" role="alert">{productError}</div>}
          <Field data-autofocus label="Nombre" value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} required disabled={savingProduct} />
          <Field label="SKU" value={draft.sku} onChange={(event) => setDraft({ ...draft, sku: event.target.value })} required disabled={savingProduct} />
          <Field label="Precio" value={draft.price} onChange={(event) => setDraft({ ...draft, price: event.target.value })} inputMode="decimal" placeholder="0.00" required disabled={savingProduct} hint="Importe decimal; Rust valida y convierte a centavos." />
          {!editingProduct && <Field label="Stock inicial" type="number" value={draft.stock} onChange={(event) => setDraft({ ...draft, stock: event.target.value })} required disabled={savingProduct} />}
          <SelectField label="Marca" value={draft.brandId} onChange={(brandId) => setDraft({ ...draft, brandId })} required disabled={savingProduct}><option value="">Selecciona una marca</option>{brands.map((brand) => <option key={brand.id} value={brand.id}>{brand.name}</option>)}</SelectField>
          <SelectField label="Categoria" value={draft.categoryId} onChange={(categoryId) => setDraft({ ...draft, categoryId })} required disabled={savingProduct}><option value="">Selecciona una categoria</option>{categories.map((category) => <option key={category.id} value={category.id}>{category.name}</option>)}</SelectField>
        </form>
      </Modal>

      <CatalogModal open={catalogOpen} brands={brands} categories={categories} onClose={() => setCatalogOpen(false)} onSessionRequired={onSessionRequired} onChanged={catalogsChanged} />
    </main>
  );
}

function CatalogModal({ open, brands, categories, onClose, onChanged, onSessionRequired }: { open: boolean; brands: Brand[]; categories: Category[]; onClose: () => void; onChanged: (message: string) => Promise<void>; onSessionRequired: () => void }) {
  const [brandId, setBrandId] = useState<number>();
  const [brandName, setBrandName] = useState("");
  const [categoryId, setCategoryId] = useState<number>();
  const [categoryName, setCategoryName] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const saveInFlight = useRef(false);

  async function save(kind: "brand" | "category", event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saveInFlight.current) return;
    saveInFlight.current = true;
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
      saved = true;
    } catch (value: unknown) {
      if (isSessionRequired(value)) onSessionRequired();
      else setError(errorMessage(value, "No se pudo guardar el catalogo."));
    } finally {
      saveInFlight.current = false;
      setSaving(false);
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
