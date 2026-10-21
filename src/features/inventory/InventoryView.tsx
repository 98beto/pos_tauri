import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import {
  createPurchase,
  createStockCount,
  getInventoryView,
  lookupInventoryMovement,
  quotePurchase,
  quoteStockCount,
  searchInventoryProducts,
} from "../../api/inventory";
import { Button, EmptyState, Field, Modal, Spinner, StatCard } from "../../components/ui";
import { errorMessage, isAppError, isSessionRequired } from "../../lib/app-error";
import { formatBaseQuantity, formatDateTime, formatMoney } from "../../lib/format";
import type {
  InventoryMovement,
  InventoryMovementPresentation,
  InventoryMovementProduct,
  InventoryView as InventoryViewData,
  CreatePurchaseInput,
  CreateStockCountInput,
  PurchaseQuote,
  StockCountQuote,
} from "../../types/inventory_movement";
import type { BaseUnit } from "../../types/product";
import type { User } from "../../types/user";
import { ProductInventoryDetail } from "./ProductInventoryDetail";
import {
  clearPendingManualMovement,
  manualMovementInput,
  readPendingManualMovement,
  replacePendingManualMovement,
  savePendingManualMovement,
  type PendingManualMovement,
  type PendingManualMovementV5,
} from "./manual-movement-intent";

type Notice = { tone: "success" | "error" | "info"; message: string };
type Props = { user: User; onNotice: (notice: Notice) => void; onSessionRequired: () => void };
type Action = "purchase" | "stock_count";
type Quote = PurchaseQuote | StockCountQuote;

const selectClass = "h-11 w-full rounded-control border border-line-strong bg-surface px-3 text-sm text-ink outline-none transition focus:border-brand focus:ring-3 focus:ring-brand/12 disabled:bg-surface-soft";
const units: Record<BaseUnit, { singular: string; plural: string }> = {
  piece: { singular: "pieza", plural: "piezas" },
  gram: { singular: "gramo", plural: "gramos" },
  milliliter: { singular: "mililitro", plural: "mililitros" },
  centimeter: { singular: "centimetro", plural: "centimetros" },
};
const submitRequests = new Map<string, Promise<InventoryMovement>>();

function submitOnce(intent: PendingManualMovementV5) {
  const key = `${intent.userId}:${intent.operationToken}`;
  const existing = submitRequests.get(key);
  if (existing) return existing;
  const input = manualMovementInput(intent);
  const request = intent.kind === "purchase" ? createPurchase(input as CreatePurchaseInput) : createStockCount(input as CreateStockCountInput);
  submitRequests.set(key, request);
  void request.then(() => submitRequests.delete(key), () => submitRequests.delete(key));
  return request;
}

function Pagination({ page, total, pageSize, onPage, label }: { page: number; total: number; pageSize: number; onPage: (page: number) => void; label: string }) {
  const pages = Math.max(1, Math.ceil(total / pageSize));
  if (pages === 1) return null;
  return <nav className="flex items-center justify-between border-t border-line p-4" aria-label={label}><Button size="sm" disabled={page === 1} onClick={() => onPage(page - 1)}>Anterior</Button><span className="text-xs font-semibold text-muted">Pagina {page} de {pages}</span><Button size="sm" disabled={page === pages} onClick={() => onPage(page + 1)}>Siguiente</Button></nav>;
}

function movementPurchasePrice(movement: InventoryMovement) {
  if (movement.purchase_unit_price_cents === null) return "Compra historica sin precio";
  const price = formatMoney(movement.purchase_unit_price_cents);
  if (movement.presentation_mode === "fixed") return `${price} por ${movement.presentation_name ?? "presentacion"}`;
  if (movement.presentation_mode === "measured" && movement.presentation_base_quantity && movement.base_unit) return `${price} por ${formatBaseQuantity(movement.presentation_base_quantity, movement.base_unit)}`;
  return `${price} por ${movement.base_unit ? units[movement.base_unit].singular : "unidad base"}`;
}

function MovementCard({ movement }: { movement: InventoryMovement }) {
  const quantity = movement.base_quantity !== null && movement.base_unit ? formatBaseQuantity(movement.base_quantity, movement.base_unit) : String(movement.quantity);
  return <article className="rounded-control border border-line p-4">
    <div className="flex flex-wrap items-start justify-between gap-2"><div><strong>{movement.product_name_snapshot}</strong><span className="block font-mono text-xs text-muted">{movement.sku_snapshot}</span></div><span className="text-xs text-muted">{formatDateTime(movement.created_at)}</span></div>
    <div className="mt-3 grid gap-1 text-sm">
      {movement.reason === "compra" && <><b>Compra · Entrada</b><span>{quantity}{movement.presentation_name ? ` · ${movement.presentation_name}` : " · Unidad base"}</span><span>{movementPurchasePrice(movement)}</span>{movement.purchase_total_cents !== null && <span>Importe: <b>{formatMoney(movement.purchase_total_cents)}</b></span>}<span>{movement.supplier ?? "Sin proveedor historico"} · {movement.document_reference ?? "Sin factura historica"}</span></>}
      {movement.reason === "ajuste" && <><b>Ajuste · {movement.type === "entrada" ? "Entrada" : "Salida"}</b><span>Stock anterior: {movement.stock_before !== null && movement.base_unit ? formatBaseQuantity(movement.stock_before, movement.base_unit) : "-"}</span><span>Stock contado: {movement.stock_after !== null && movement.base_unit ? formatBaseQuantity(movement.stock_after, movement.base_unit) : "-"}</span><span>Diferencia: {movement.type === "entrada" ? "+" : "-"}{quantity}</span><span>Motivo: {movement.note ?? "Sin motivo historico"}</span></>}
      {movement.reason === "venta" && <><b>Venta · Salida</b><span>Cantidad retirada: {quantity}</span><span>Ticket: {movement.ticket_reference ?? "Sin referencia"}</span></>}
      {movement.reason === "compra" && movement.note && <span>Nota: {movement.note}</span>}
    </div>
  </article>;
}

export function InventoryView({ user, onNotice, onSessionRequired }: Props) {
  const [view, setView] = useState<InventoryViewData>();
  const [tab, setTab] = useState<"products" | "movements">("products");
  const [productSearch, setProductSearch] = useState("");
  const [productUnit, setProductUnit] = useState("");
  const [movementSearch, setMovementSearch] = useState("");
  const [reason, setReason] = useState("");
  const [movementType, setMovementType] = useState("");
  const [productPage, setProductPage] = useState(1);
  const [movementPage, setMovementPage] = useState(1);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [detailId, setDetailId] = useState<number>();

  const [action, setAction] = useState<Action>();
  const [products, setProducts] = useState<InventoryMovementProduct[]>([]);
  const [productsLoading, setProductsLoading] = useState(false);
  const [selectorSearch, setSelectorSearch] = useState("");
  const [productId, setProductId] = useState("");
  const [presentationId, setPresentationId] = useState("");
  const [quantity, setQuantity] = useState("");
  const [purchasePrice, setPurchasePrice] = useState("");
  const [supplier, setSupplier] = useState("");
  const [documentReference, setDocumentReference] = useState("");
  const [note, setNote] = useState("");
  const [countedStock, setCountedStock] = useState("");
  const [adjustmentReason, setAdjustmentReason] = useState("");
  const [quote, setQuote] = useState<Quote>();
  const [formError, setFormError] = useState("");
  const [fieldError, setFieldError] = useState<{ field?: string; message: string }>();
  const [busy, setBusy] = useState(false);
  const [canRequote, setCanRequote] = useState(false);
  const [draftToken, setDraftToken] = useState<string>();
  const [pending, setPending] = useState<PendingManualMovement | null>(() => readPendingManualMovement(user.id));
  const [legacyApplied, setLegacyApplied] = useState(false);
  const viewRequest = useRef(0);
  const quoteRequest = useRef(0);
  const activeQuote = useRef<number | undefined>(undefined);
  const selectorRequest = useRef(0);
  const returnFocusProduct = useRef<number | undefined>(undefined);
  const mounted = useRef(true);
  const session = useRef({ userId: user.id, generation: 0 });
  if (session.current.userId !== user.id) {
    session.current = { userId: user.id, generation: session.current.generation + 1 };
    viewRequest.current += 1; quoteRequest.current += 1; selectorRequest.current += 1;
    activeQuote.current = undefined;
  }

  function isCurrentSession(generation: number, userId: number) {
    return mounted.current && session.current.generation === generation && session.current.userId === userId;
  }

  async function loadView() {
    const { generation, userId } = session.current;
    if (!isCurrentSession(generation, userId)) return;
    const request = ++viewRequest.current;
    const isCurrent = () => isCurrentSession(generation, userId) && request === viewRequest.current;
    setLoading(true); setError("");
    try {
      const next = await getInventoryView({
        product_search: productSearch || undefined, product_page: productPage,
        base_unit: (productUnit || undefined) as BaseUnit | undefined,
        movement_search: movementSearch || undefined, movement_page: movementPage,
        reason: (reason || undefined) as InventoryMovement["reason"] | undefined,
        type: (movementType || undefined) as InventoryMovement["type"] | undefined,
      });
      if (isCurrent()) setView(next);
    } catch (value) {
      if (!isCurrent()) return;
      if (isSessionRequired(value)) onSessionRequired();
      else setError(errorMessage(value, "No se pudo consultar el inventario."));
    } finally { if (isCurrent()) setLoading(false); }
  }

  useEffect(() => { mounted.current = true; return () => { mounted.current = false; session.current.generation += 1; viewRequest.current += 1; quoteRequest.current += 1; selectorRequest.current += 1; }; }, []);
  useEffect(() => {
    const { generation, userId } = session.current;
    setLoading(true); setError("");
    const timer = window.setTimeout(() => { if (isCurrentSession(generation, userId)) void loadView(); }, 250);
    return () => { window.clearTimeout(timer); viewRequest.current += 1; };
  }, [user.id, productSearch, productUnit, productPage, movementSearch, reason, movementType, movementPage]);
  useEffect(() => {
    resetForm(); setBusy(false); setPending(readPendingManualMovement(user.id)); setAction(undefined); setDetailId(undefined); setView(undefined);
  }, [user.id]);
  useEffect(() => {
    const { generation, userId } = session.current;
    const request = ++selectorRequest.current;
    const isCurrent = () => isCurrentSession(generation, userId) && request === selectorRequest.current;
    if (!action || productId) { setProductsLoading(false); return; }
    const timer = window.setTimeout(async () => {
      if (!isCurrent()) return;
      setProductsLoading(true); setProducts([]); setFormError("");
      try {
        const result = await searchInventoryProducts(selectorSearch || undefined, 1);
        if (isCurrent()) setProducts(result.items);
      } catch (value) {
        if (!isCurrent()) return;
        if (isSessionRequired(value)) { setAction(undefined); resetForm(); onSessionRequired(); }
        else setFormError(errorMessage(value, "No se pudieron cargar los productos."));
      } finally { if (isCurrent()) setProductsLoading(false); }
    }, 250);
    return () => { window.clearTimeout(timer); if (request === selectorRequest.current) selectorRequest.current += 1; };
  }, [user.id, action, selectorSearch, productId]);
  useEffect(() => {
    if (detailId !== undefined || returnFocusProduct.current === undefined) return;
    const { generation, userId } = session.current;
    const product = returnFocusProduct.current;
    const frame = window.requestAnimationFrame(() => {
      if (!isCurrentSession(generation, userId)) return;
      document.getElementById(`inventory-product-history-${product}`)?.focus();
      returnFocusProduct.current = undefined;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [detailId]);

  function resetForm() {
    invalidateQuote(); setCanRequote(false);
    quoteRequest.current += 1; selectorRequest.current += 1;
    setProductId(""); setPresentationId(""); setQuantity(""); setPurchasePrice(""); setSupplier(""); setDocumentReference(""); setNote("");
    setCountedStock(""); setAdjustmentReason(""); setQuote(undefined); setFormError(""); setFieldError(undefined); setDraftToken(undefined); setSelectorSearch(""); setProducts([]); setProductsLoading(false); setLegacyApplied(false);
  }
  function openAction(next: Action, product?: InventoryMovementProduct) {
    const stored = readPendingManualMovement(user.id);
    if (pending || stored || busy) { if (stored) setPending(stored); return; }
    resetForm(); setAction(next);
    if (product) selectProduct(product);
  }
  function closeAction() { if (!busy) { setAction(undefined); resetForm(); } }
  function invalidateQuote() {
    quoteRequest.current += 1;
    if (activeQuote.current !== undefined) { activeQuote.current = undefined; setBusy(false); }
    setQuote(undefined); setFieldError(undefined);
  }
  function changeView(setter: (value: string) => void, value: string, resetPage: (page: number) => void) {
    setLoading(true); setError(""); resetPage(1); setter(value);
  }
  function selectProduct(product: InventoryMovementProduct) {
    setProducts((current) => current.some((item) => item.id === product.id) ? current : [product, ...current]);
    setProductId(String(product.id));
    const basePiece = product.base_unit === "piece" ? product.presentations.find((item) => item.is_active && item.mode === "fixed" && item.base_quantity === 1) : undefined;
    setPresentationId(basePiece ? String(basePiece.id) : ""); invalidateQuote();
  }
  function handleTabKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    const { generation, userId } = session.current;
    let next: "products" | "movements" | undefined;
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") next = tab === "products" ? "movements" : "products";
    else if (event.key === "Home") next = "products";
    else if (event.key === "End") next = "movements";
    if (!next) return;
    event.preventDefault(); setTab(next);
    window.requestAnimationFrame(() => { if (isCurrentSession(generation, userId)) document.getElementById(`inventory-${next}-tab`)?.focus(); });
  }

  async function confirmIntent(intent: PendingManualMovementV5, generation = session.current.generation) {
    if (!isCurrentSession(generation, intent.userId)) return;
    setBusy(true); setFormError("");
    try {
      await submitOnce(intent);
      if (!isCurrentSession(generation, intent.userId)) return;
      const cleared = clearPendingManualMovement(intent.userId, intent.operationToken);
      if (cleared !== "cleared" && cleared !== "absent") throw new Error("pending-cleanup");
      setPending(null); setAction(undefined); resetForm();
      onNotice({ tone: "success", message: intent.kind === "purchase" ? "Compra registrada." : "Existencias ajustadas." });
      if (!isCurrentSession(generation, intent.userId)) return;
      await loadView();
      if (!isCurrentSession(generation, intent.userId)) return;
    } catch (value) {
      if (!isCurrentSession(generation, intent.userId)) return;
      if (isSessionRequired(value)) { setAction(undefined); onSessionRequired(); return; }
      if (isAppError(value) && ["QUOTE_CHANGED", "INVENTORY_QUOTE_REQUIRED"].includes(value.code)) {
        setCanRequote(true); setQuote(undefined); setFormError("La cotizacion ya no es vigente. Cotiza nuevamente antes de confirmar.");
        return "stale";
      } else setFormError(value instanceof Error && value.message === "pending-cleanup" ? "La operacion fue aplicada, pero no se pudo limpiar la recuperacion local." : errorMessage(value, "No se recibio la confirmacion. Puedes reintentar de forma segura."));
    } finally { if (isCurrentSession(generation, intent.userId)) setBusy(false); }
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    const { generation, userId } = session.current;
    if (!isCurrentSession(generation, userId)) return;
    if (!action || !productId || busy) { if (!productId) setFormError("Selecciona un producto."); return; }
    setFormError(""); setFieldError(undefined);
    if (pending?.version === 5 && !canRequote) { await confirmIntent(pending); return; }
    if (!quote) {
      const request = ++quoteRequest.current; activeQuote.current = request; setBusy(true);
      const isCurrent = () => isCurrentSession(generation, userId) && request === quoteRequest.current;
      try {
        const next = action === "purchase"
          ? await quotePurchase({ product_id: Number(productId), presentation_id: presentationId ? Number(presentationId) : null, quantity, purchase_unit_price: purchasePrice, supplier, document_reference: documentReference, note: note || null })
          : await quoteStockCount({ product_id: Number(productId), counted_stock: countedStock, adjustment_reason: adjustmentReason });
        if (isCurrent()) setQuote(next);
      } catch (value) {
        if (!isCurrent()) return;
        if (isSessionRequired(value)) { setAction(undefined); onSessionRequired(); }
        else {
          const message = errorMessage(value, "No se pudo calcular la operacion.");
          if (isAppError(value) && value.field) {
            setFieldError({ field: value.field, message });
            window.requestAnimationFrame(() => { if (isCurrent()) document.querySelector<HTMLInputElement>('#inventory-action-form [aria-invalid="true"]')?.focus(); });
          } else setFormError(message);
        }
      } finally { if (isCurrent() && activeQuote.current === request) { activeQuote.current = undefined; setBusy(false); } }
      return;
    }
    const token = draftToken ?? crypto.randomUUID();
    const intent: PendingManualMovementV5 = action === "purchase" ? {
      version: 5, kind: "purchase", userId: user.id, operationToken: token, productId: Number(productId), presentationId: presentationId ? Number(presentationId) : null,
      quantity, purchaseUnitPrice: purchasePrice, supplier, documentReference, note: note || null, quoteFingerprint: quote.quote_fingerprint,
    } : {
      version: 5, kind: "stock_count", userId: user.id, operationToken: token, productId: Number(productId), countedStock, adjustmentReason, quoteFingerprint: quote.quote_fingerprint,
    };
    try {
      if (pending && draftToken) {
        if (!replacePendingManualMovement(pending, intent)) { setFormError("El registro pendiente cambio. Vuelve a abrirlo."); return; }
      } else savePendingManualMovement(intent);
      setPending(intent); setDraftToken(token); setCanRequote(false); await confirmIntent(intent);
    } catch { if (isCurrentSession(generation, userId)) setFormError("No se pudo guardar la operacion pendiente. Intenta de nuevo."); }
  }

  async function recoverPending() {
    const generation = session.current.generation;
    if (!pending || busy || !isCurrentSession(generation, pending.userId)) return;
    setBusy(true); setFormError("");
    try {
      const applied = await lookupInventoryMovement({ operation_token: pending.operationToken });
      if (!isCurrentSession(generation, pending.userId)) return;
      if (applied) {
        const cleared = clearPendingManualMovement(pending.userId, pending.operationToken);
        if (cleared !== "cleared" && cleared !== "absent") throw new Error("No se pudo limpiar la recuperacion local.");
        setPending(null); setAction(undefined); resetForm(); setLegacyApplied(pending.version !== 5);
        onNotice({ tone: "success", message: "La operacion pendiente ya estaba aplicada; no se duplico." });
        if (!isCurrentSession(generation, pending.userId)) return;
        await loadView();
        if (!isCurrentSession(generation, pending.userId)) return;
        return;
      }
      if (pending.version === 5) {
        const result = await confirmIntent(pending, generation);
        if (!isCurrentSession(generation, pending.userId)) return;
        if (result !== "stale") return;
      }
      setBusy(true);
      const result = await searchInventoryProducts(undefined, 1);
      if (!isCurrentSession(generation, pending.userId)) return;
      let product = result.items.find((item) => item.id === pending.productId);
      for (let page = 2; !product && (page - 1) * result.page_size < result.total; page += 1) {
        const next = await searchInventoryProducts(undefined, page);
        if (!isCurrentSession(generation, pending.userId)) return;
        product = next.items.find((item) => item.id === pending.productId);
      }
      if (!product) { setFormError("El producto del registro pendiente ya no esta disponible."); return; }
      resetForm(); setDraftToken(pending.operationToken); setProducts([product]); setProductId(String(product.id));
      if (pending.version === 5) {
        setAction(pending.kind); setCanRequote(true);
        if (pending.kind === "purchase") {
          setPresentationId(pending.presentationId === null ? "" : String(pending.presentationId));
          setQuantity(pending.quantity); setPurchasePrice(pending.purchaseUnitPrice); setSupplier(pending.supplier);
          setDocumentReference(pending.documentReference); setNote(pending.note ?? "");
        } else { setCountedStock(pending.countedStock); setAdjustmentReason(pending.adjustmentReason); }
        setFormError("La cotizacion ya no es vigente. Cotiza nuevamente antes de confirmar.");
        return;
      }
      setAction(pending.reason === "compra" ? "purchase" : "stock_count");
      setFormError("El formato anterior no puede confirmarse. Recaptura los datos y solicita una nueva cotizacion.");
    } catch (value) {
      if (!isCurrentSession(generation, pending.userId)) return;
      if (isSessionRequired(value)) onSessionRequired(); else setFormError(errorMessage(value, "No se pudo consultar la operacion pendiente."));
    } finally { if (isCurrentSession(generation, pending.userId)) setBusy(false); }
  }

  const selectedProduct = products.find((item) => item.id === Number(productId)) ?? view?.products.find((item) => item.id === Number(productId));
  const presentations = selectedProduct?.presentations.filter((item) => item.is_active && (selectedProduct.base_unit === "piece" || item.mode === "measured")) ?? [];
  const selectedPresentation = presentations.find((item) => item.id === Number(presentationId));
  const captureOptions: Array<InventoryMovementPresentation | null> = [null, ...presentations].filter((item, _index, all) => !(item === null && selectedProduct?.base_unit === "piece" && all.some((candidate) => candidate?.mode === "fixed" && candidate.base_quantity === 1)));
  const priceLabel = selectedPresentation?.mode === "fixed" ? `Precio de compra por ${selectedPresentation.name}` : selectedPresentation?.mode === "measured" && selectedProduct ? `Precio de compra por ${formatBaseQuantity(selectedPresentation.base_quantity, selectedProduct.base_unit)}` : `Precio de compra por ${selectedProduct ? units[selectedProduct.base_unit].singular : "unidad base"}`;
  const pageSize = view?.page_size ?? 20;
  const formLocked = busy || Boolean(pending?.version === 5 && !canRequote);

  if (detailId) return <main id="main-content" tabIndex={-1} className="p-4 outline-none sm:p-6 min-[900px]:p-8"><ProductInventoryDetail productId={detailId} actionsDisabled={Boolean(pending) || busy} onBack={() => setDetailId(undefined)} onSessionRequired={onSessionRequired} onPurchase={() => { const product = view?.products.find((item) => item.id === detailId); openAction("purchase", product); setDetailId(undefined); }} onStockCount={() => { const product = view?.products.find((item) => item.id === detailId); openAction("stock_count", product); setDetailId(undefined); }} /></main>;

  return <main id="main-content" tabIndex={-1} className="p-4 outline-none sm:p-6 min-[900px]:p-8">
    {!action && formError && <div role="alert" className="mb-4 text-sm text-danger">{formError}</div>}
    {pending && <section className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-card border border-warning/30 bg-warning-soft p-4" role="status"><div><strong>Operacion pendiente</strong><p className="text-xs text-muted">{pending.version === 5 ? "Puedes reintentar el envio sin duplicar la operacion." : "Se consultara el token antes de exigir una nueva captura."}</p></div><Button onClick={() => void recoverPending()} loading={busy}>{pending.version === 5 ? "Reintentar envio" : "Revisar operacion"}</Button></section>}
    {legacyApplied && <p className="mb-4 rounded-control border border-success/30 bg-success-soft p-3 text-sm" role="status">La operacion legacy ya estaba aplicada.</p>}
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-5" aria-busy={loading}><StatCard label="Productos" value={view?.stats.product_count ?? "-"} /><StatCard label="Existencia en piezas" value={view ? formatBaseQuantity(view.stats.piece_stock, "piece") : "-"} /><StatCard label="Existencia por peso" value={view ? formatBaseQuantity(view.stats.gram_stock, "gram") : "-"} /><StatCard label="Existencia por volumen" value={view ? formatBaseQuantity(view.stats.milliliter_stock, "milliliter") : "-"} /><StatCard label="Existencia por longitud" value={view ? formatBaseQuantity(view.stats.centimeter_stock, "centimeter") : "-"} /></div>
    <section className="mt-5 overflow-hidden rounded-card border border-line bg-surface">
      <header className="flex flex-wrap items-end justify-between gap-3 border-b border-line p-4 sm:p-5"><div><h1 className="font-heading text-2xl font-bold">Inventario</h1><p className="mt-1 text-xs text-muted">Existencias fisicas y documentos de compra.</p></div><div className="flex flex-wrap gap-2"><Button variant="primary" onClick={() => openAction("purchase")} disabled={Boolean(pending)}>Registrar compra</Button><Button onClick={() => openAction("stock_count")} disabled={Boolean(pending)}>Ajustar existencias</Button></div></header>
      <div className="border-b border-line px-4 pt-3" role="tablist" aria-label="Secciones de inventario"><button id="inventory-products-tab" role="tab" aria-selected={tab === "products"} tabIndex={tab === "products" ? 0 : -1} className={`border-b-2 px-4 py-3 text-sm font-bold ${tab === "products" ? "border-brand text-brand" : "border-transparent text-muted"}`} onClick={() => setTab("products")} onKeyDown={handleTabKeyDown}>Productos</button><button id="inventory-movements-tab" role="tab" aria-selected={tab === "movements"} tabIndex={tab === "movements" ? 0 : -1} className={`border-b-2 px-4 py-3 text-sm font-bold ${tab === "movements" ? "border-brand text-brand" : "border-transparent text-muted"}`} onClick={() => setTab("movements")} onKeyDown={handleTabKeyDown}>Movimientos</button></div>
      {error && <div className="m-4 flex items-center justify-between gap-3 rounded-control border border-danger/25 bg-danger-soft p-3 text-sm text-danger" role="alert"><span>{error}</span><Button size="sm" onClick={() => void loadView()}>Reintentar</Button></div>}
      {loading && view && <p className="mx-4 mt-3 text-xs font-semibold text-muted" role="status">Actualizando; se muestran temporalmente los resultados anteriores.</p>}
      {error && view && <p className="mx-4 mt-3 text-xs font-semibold text-muted">Se muestran los resultados anteriores; no corresponden a los filtros actuales.</p>}
      <div role="tabpanel" aria-label="Productos" aria-busy={loading || undefined} hidden={tab !== "products"}>
        <div className="grid gap-3 border-b border-line bg-surface-soft/60 p-4 sm:grid-cols-2"><Field label="Buscar producto" value={productSearch} onChange={(event) => changeView(setProductSearch, event.target.value, setProductPage)} /><label className="grid gap-2 text-[0.625rem] font-extrabold tracking-wider text-muted uppercase">Unidad base<select className={selectClass} value={productUnit} onChange={(event) => changeView(setProductUnit, event.target.value, setProductPage)}><option value="">Todas</option><option value="piece">Pieza</option><option value="gram">Gramo</option><option value="milliliter">Mililitro</option><option value="centimeter">Centimetro</option></select></label></div>
        {!view && loading ? <div className="grid min-h-64 place-items-center"><Spinner className="size-6 text-brand" /></div> : view?.products.length ? <div className="grid gap-3 p-4 md:grid-cols-2 xl:grid-cols-3">{view.products.map((product) => <article key={product.id} className="rounded-control border border-line p-4"><strong>{product.name}</strong><span className="block font-mono text-xs text-muted">{product.sku}</span><span className="mt-3 block text-sm">Stock: <b>{formatBaseQuantity(product.stock, product.base_unit)}</b></span><div className="mt-4 flex flex-wrap gap-2"><Button size="sm" variant="primary" onClick={() => openAction("purchase", product)} disabled={Boolean(pending)}>Comprar</Button><Button size="sm" onClick={() => openAction("stock_count", product)} disabled={Boolean(pending)}>Contar</Button><Button id={`inventory-product-history-${product.id}`} size="sm" onClick={() => { returnFocusProduct.current = product.id; setDetailId(product.id); }}>Ver historial</Button></div></article>)}</div> : !loading && <EmptyState title="No hay productos" description="Cambia los filtros para ampliar los resultados." icon="□" />}
        {view && !loading && !error && <Pagination label="Paginacion de productos" page={productPage} total={view.product_total} pageSize={pageSize} onPage={(page) => { setLoading(true); setError(""); setProductPage(page); }} />}
      </div>
      <div role="tabpanel" aria-label="Movimientos" aria-busy={loading || undefined} hidden={tab !== "movements"}>
        <div className="grid gap-3 border-b border-line bg-surface-soft/60 p-4 sm:grid-cols-3"><Field label="Buscar movimiento" value={movementSearch} onChange={(event) => changeView(setMovementSearch, event.target.value, setMovementPage)} /><label className="grid gap-2 text-[0.625rem] font-extrabold tracking-wider text-muted uppercase">Motivo<select className={selectClass} value={reason} onChange={(event) => changeView(setReason, event.target.value, setMovementPage)}><option value="">Todos</option><option value="compra">Compra</option><option value="ajuste">Ajuste</option><option value="venta">Venta</option></select></label><label className="grid gap-2 text-[0.625rem] font-extrabold tracking-wider text-muted uppercase">Direccion<select className={selectClass} value={movementType} onChange={(event) => changeView(setMovementType, event.target.value, setMovementPage)}><option value="">Todas</option><option value="entrada">Entrada</option><option value="salida">Salida</option></select></label></div>
        {!view && loading ? <div className="grid min-h-64 place-items-center"><Spinner className="size-6 text-brand" /></div> : view?.movements.length ? <div className="grid gap-3 p-4 lg:grid-cols-2">{view.movements.map((movement) => <MovementCard key={movement.id} movement={movement} />)}</div> : !loading && <EmptyState title="No hay movimientos" description="Los movimientos apareceran despues de una compra, ajuste o venta." icon="↕" />}
        {view && !loading && !error && <Pagination label="Paginacion de movimientos" page={movementPage} total={view.movement_total} pageSize={pageSize} onPage={(page) => { setLoading(true); setError(""); setMovementPage(page); }} />}
      </div>
    </section>

    <Modal open={Boolean(action)} onClose={closeAction} closeDisabled={busy} title={action === "purchase" ? "Registrar compra" : "Ajustar existencias"} description={action === "purchase" ? "Captura la compra y revisa su importe antes de confirmar." : "Captura el stock fisico absoluto en la unidad base."} size="md" footer={<><Button onClick={closeAction} disabled={busy}>Cancelar</Button><Button form="inventory-action-form" type="submit" variant="primary" loading={busy}>{quote ? action === "purchase" ? "Confirmar compra" : "Confirmar ajuste" : "Calcular resumen"}</Button></>}>
      <form id="inventory-action-form" className="grid gap-4" noValidate onSubmit={(event) => void submit(event)}>
        <fieldset disabled={formLocked} className="contents">
        {formError && <div className="rounded-control border border-danger/25 bg-danger-soft p-3 text-sm text-danger" role="alert">{formError}</div>}
        {!selectedProduct ? <div className="grid gap-2"><Field data-autofocus label="Buscar producto" value={selectorSearch} onChange={(event) => setSelectorSearch(event.target.value)} disabled={busy} />{productsLoading ? <p className="text-sm text-muted" role="status">Buscando productos...</p> : <ul aria-label="Productos" className="max-h-56 overflow-y-auto rounded-control border border-line">{products.map((product) => <li key={product.id} className="border-b border-line last:border-0"><button type="button" className="block w-full p-3 text-left hover:bg-surface-soft" onClick={() => selectProduct(product)}><strong>{product.name}</strong><span className="block text-xs text-muted">{product.sku} · {formatBaseQuantity(product.stock, product.base_unit)}</span></button></li>)}</ul>}</div> : <section className="rounded-control border border-line bg-surface-soft p-4" aria-label="Producto seleccionado"><strong>{selectedProduct.name}</strong><span className="block font-mono text-xs text-muted">{selectedProduct.sku}</span><span className="mt-2 block text-sm">Stock actual: <b>{formatBaseQuantity(selectedProduct.stock, selectedProduct.base_unit)}</b></span></section>}
        {action === "purchase" && selectedProduct && <>
          <fieldset className="grid gap-2"><legend className="text-[0.625rem] font-extrabold tracking-wider text-muted uppercase">Forma o presentacion</legend><div className="grid gap-2 sm:grid-cols-2">{captureOptions.map((presentation) => { const value = presentation ? String(presentation.id) : ""; const label = presentation?.name ?? `Captura directa en ${units[selectedProduct.base_unit].plural}`; return <label key={value || "base"} className={`rounded-control border p-3 text-sm ${presentationId === value ? "border-brand bg-brand/5" : "border-line"}`}><input className="mr-2" type="radio" name="capture" checked={presentationId === value} onChange={() => { setPresentationId(value); invalidateQuote(); }} />{label}<span className="mt-1 block text-xs text-muted">{presentation?.mode === "fixed" ? `${formatBaseQuantity(presentation.base_quantity, selectedProduct.base_unit)} por presentacion` : presentation ? `Precio basado en ${formatBaseQuantity(presentation.base_quantity, selectedProduct.base_unit)}` : `Precio por ${units[selectedProduct.base_unit].singular}`}</span></label>; })}</div></fieldset>
          <Field data-autofocus label={selectedPresentation?.mode === "fixed" ? "Cantidad de presentaciones" : `Cantidad en ${units[selectedProduct.base_unit].plural}`} value={quantity} onChange={(event) => { setQuantity(event.target.value); invalidateQuote(); }} inputMode="numeric" required disabled={busy} error={fieldError?.field === "quantity" ? fieldError.message : undefined} />
          <Field label={priceLabel} value={purchasePrice} onChange={(event) => { setPurchasePrice(event.target.value); invalidateQuote(); }} inputMode="decimal" placeholder="0.00" required disabled={busy} hint="Precio de compra, no precio al cliente." error={fieldError?.field === "purchase_unit_price" ? fieldError.message : undefined} />
          <div className="grid gap-4 sm:grid-cols-2"><Field label="Proveedor" value={supplier} onChange={(event) => { setSupplier(event.target.value); invalidateQuote(); }} required disabled={busy} error={fieldError?.field === "supplier" ? fieldError.message : undefined} /><Field label="Factura o referencia" value={documentReference} onChange={(event) => { setDocumentReference(event.target.value); invalidateQuote(); }} required disabled={busy} error={fieldError?.field === "document_reference" ? fieldError.message : undefined} /></div>
          <label className="grid gap-2 text-[0.625rem] font-extrabold tracking-wider text-muted uppercase">Nota opcional<textarea className={selectClass + " min-h-20 py-2"} value={note} onChange={(event) => { setNote(event.target.value); invalidateQuote(); }} disabled={busy} /></label>
        </>}
        {action === "stock_count" && selectedProduct && <><div className="rounded-control border border-brand/20 bg-brand/5 p-3 text-sm">Unidad de conteo: <b>{units[selectedProduct.base_unit].plural}</b>. No uses presentaciones ni datos comerciales.</div><Field data-autofocus label={`Stock contado en ${units[selectedProduct.base_unit].plural}`} value={countedStock} onChange={(event) => { setCountedStock(event.target.value); invalidateQuote(); }} inputMode="numeric" required disabled={busy} error={fieldError?.field === "counted_stock" ? fieldError.message : undefined} /><Field label="Motivo del ajuste" value={adjustmentReason} onChange={(event) => { setAdjustmentReason(event.target.value); invalidateQuote(); }} required disabled={busy} error={fieldError?.field === "adjustment_reason" ? fieldError.message : undefined} /></>}
        {quote && ("purchase_total_cents" in quote ? <section className="rounded-control border border-brand/25 bg-brand/5 p-4 text-sm" role="status"><strong>Resumen de compra</strong><dl className="mt-2 grid gap-1"><div>Stock actual: <b>{formatBaseQuantity(quote.stock_before, quote.base_unit)}</b></div><div>Cantidad capturada: <b>{quote.entered_quantity} · {quote.capture_name}</b></div><div>Cantidad base agregada: <b>{formatBaseQuantity(quote.base_quantity, quote.base_unit)}</b></div><div>Stock resultante: <b>{formatBaseQuantity(quote.stock_after, quote.base_unit)}</b></div><div>Precio: <b>{formatMoney(quote.purchase_unit_price_cents)} · {quote.purchase_price_description}</b></div><div>Importe total: <b>{formatMoney(quote.purchase_total_cents)}</b></div><div>Proveedor / factura: <b>{quote.supplier} · {quote.document_reference}</b></div></dl></section> : <section className="rounded-control border border-brand/25 bg-brand/5 p-4 text-sm" role="status"><strong>Resumen del conteo</strong><dl className="mt-2 grid gap-1"><div>Stock registrado: <b>{formatBaseQuantity(quote.stock_before, quote.base_unit)}</b></div><div>Stock contado: <b>{formatBaseQuantity(quote.counted_stock, quote.base_unit)}</b></div><div>Diferencia: <b>{quote.difference > 0 ? "+" : ""}{formatBaseQuantity(quote.difference, quote.base_unit)}</b></div><div>Resultado: <b>{quote.type === "entrada" ? "Entrada" : "Salida"}</b></div><div>Motivo: <b>{quote.adjustment_reason}</b></div></dl></section>)}
        </fieldset>
      </form>
    </Modal>
  </main>;
}
