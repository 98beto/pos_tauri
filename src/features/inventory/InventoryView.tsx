import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { createInventoryMovement, getInventoryView, quoteInventoryMovement, searchInventoryProducts } from "../../api/inventory";
import { Button, EmptyState, Field, Modal, Spinner, StatCard } from "../../components/ui";
import { errorMessage, isAppError, isSessionRequired } from "../../lib/app-error";
import { formatBaseQuantity, formatDateTime, formatMoney } from "../../lib/format";
import type { InventoryMovement, InventoryMovementPresentation, InventoryMovementProduct, InventoryMovementQuote, InventoryView as InventoryViewData } from "../../types/inventory_movement";
import type { BaseUnit, InventoryCostStatus } from "../../types/product";
import type { User } from "../../types/user";
import { ProductInventoryDetail } from "./ProductInventoryDetail";
import { clearPendingManualMovement, manualMovementInput, readPendingManualMovement, replacePendingManualMovement, savePendingManualMovement, type PendingManualMovement, type PendingManualMovementV4 } from "./manual-movement-intent";

type Notice = { tone: "success" | "error" | "info"; message: string };
type Props = { user: User; onNotice: (notice: Notice) => void; onSessionRequired: () => void };
type Tab = "products" | "movements";
type Operation = "purchase" | "adjustment-in" | "adjustment-out" | "valuation";
type LoadResult = "success" | "error" | "session" | "stale";

const operations: Record<Operation, { reason: "compra" | "ajuste" | "valoracion"; type: "entrada" | "salida" | "valoracion" }> = {
  purchase: { reason: "compra", type: "entrada" },
  "adjustment-in": { reason: "ajuste", type: "entrada" },
  "adjustment-out": { reason: "ajuste", type: "salida" },
  valuation: { reason: "valoracion", type: "valoracion" },
};
const selectClass = "h-11 w-full rounded-control border border-line-strong bg-surface px-3 text-sm text-ink outline-none transition focus:border-brand focus:ring-3 focus:ring-brand/12 disabled:bg-surface-soft";
const unitLabels: Record<BaseUnit, string> = { piece: "Piezas", gram: "Gramos", milliliter: "Mililitros", centimeter: "Centimetros" };
const directQuantityLabels: Record<BaseUnit, string> = { piece: "Cantidad en piezas", gram: "Cantidad en gramos", milliliter: "Cantidad en mililitros", centimeter: "Cantidad en centimetros" };
const directCaptureTitles: Record<BaseUnit, string> = { piece: "Piezas individuales", gram: "Captura directa en gramos", milliliter: "Captura directa en mililitros", centimeter: "Captura directa en centimetros" };
const directCaptureDescriptions: Record<BaseUnit, string> = { piece: "La cantidad se registra directamente en piezas.", gram: "Captura directamente los gramos.", milliliter: "Captura directamente los mililitros.", centimeter: "Captura directamente los centimetros." };
const reasonLabels: Record<InventoryMovement["reason"], string> = { compra: "Compra", venta: "Venta", ajuste: "Ajuste", valoracion: "Valoracion" };
const movementRequests = new Map<string, Promise<InventoryMovement>>();

function submitMovementOnce(intent: PendingManualMovementV4) {
  const key = `${intent.userId}:${intent.operationToken}`;
  const active = movementRequests.get(key);
  if (active) return active;
  const request = createInventoryMovement(manualMovementInput(intent));
  movementRequests.set(key, request);
  void request.then(() => movementRequests.delete(key), () => movementRequests.delete(key));
  return request;
}

function operationFor(intent: PendingManualMovement): Operation {
  if (intent.reason === "compra") return "purchase";
  if (intent.reason === "valoracion") return "valuation";
  return intent.type === "entrada" ? "adjustment-in" : "adjustment-out";
}

function Pagination({ label, page, total, pageSize, onPage }: { label: string; page: number; total: number; pageSize: number; onPage: (page: number) => void }) {
  const pages = Math.max(1, Math.ceil(total / pageSize));
  if (pages <= 1) return null;
  return <nav className="flex items-center justify-between gap-3 border-t border-line px-4 py-3" aria-label={label}><Button size="sm" disabled={page <= 1} onClick={() => onPage(page - 1)}>Anterior</Button><span className="text-xs font-semibold text-muted">Pagina {page} de {pages}</span><Button size="sm" disabled={page >= pages} onClick={() => onPage(page + 1)}>Siguiente</Button></nav>;
}

export function InventoryView({ user, onNotice, onSessionRequired }: Props) {
  const [view, setView] = useState<InventoryViewData>();
  const [tab, setTab] = useState<Tab>("products");
  const [productSearch, setProductSearch] = useState("");
  const [productUnit, setProductUnit] = useState("");
  const [costStatus, setCostStatus] = useState("");
  const [productPage, setProductPage] = useState(1);
  const [movementSearch, setMovementSearch] = useState("");
  const [reason, setReason] = useState("");
  const [movementType, setMovementType] = useState("");
  const [movementPage, setMovementPage] = useState(1);
  const [selectedProductId, setSelectedProductId] = useState<number>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const [products, setProducts] = useState<InventoryMovementProduct[]>([]);
  const [productsLoading, setProductsLoading] = useState(false);
  const [productsError, setProductsError] = useState("");
  const [selectorSearch, setSelectorSearch] = useState("");
  const [selectorPage, setSelectorPage] = useState(1);
  const [selectorHasMore, setSelectorHasMore] = useState(false);
  const [selectorOpen, setSelectorOpen] = useState(false);
  const [activeProductIndex, setActiveProductIndex] = useState(-1);
  const [movementOpen, setMovementOpen] = useState(false);
  const [productId, setProductId] = useState("");
  const [presentationId, setPresentationId] = useState("");
  const [operation, setOperation] = useState<Operation>("purchase");
  const [quantity, setQuantity] = useState("");
  const [unitPrice, setUnitPrice] = useState("");
  const [supplier, setSupplier] = useState("");
  const [documentReference, setDocumentReference] = useState("");
  const [note, setNote] = useState("");
  const [movementError, setMovementError] = useState("");
  const [quantityError, setQuantityError] = useState("");
  const [unitPriceError, setUnitPriceError] = useState("");
  const [saving, setSaving] = useState(false);
  const [quoteLoading, setQuoteLoading] = useState(false);
  const [quote, setQuote] = useState<InventoryMovementQuote>();
  const [draftOperationToken, setDraftOperationToken] = useState<string>();
  const [pendingIntent, setPendingIntent] = useState<PendingManualMovement | null>(() => readPendingManualMovement(user.id));
  const [intentCanBeDiscarded, setIntentCanBeDiscarded] = useState(false);
  const [cleanupRequired, setCleanupRequired] = useState(false);
  const [discardOpen, setDiscardOpen] = useState(false);
  const [discardError, setDiscardError] = useState("");

  const viewRequest = useRef(0);
  const productsRequest = useRef(0);
  const productsRetry = useRef({ page: 1, append: false, search: "" });
  const selectorDismissed = useRef(false);
  const quoteRequest = useRef(0);
  const saveInFlight = useRef(false);
  const mounted = useRef(true);
  const lifetime = useRef(0);
  const quantityRef = useRef<HTMLInputElement>(null);
  const unitPriceRef = useRef<HTMLInputElement>(null);
  const selectorInputRef = useRef<HTMLInputElement>(null);
  const returnFocusId = useRef<string | undefined>(undefined);

  async function loadView(): Promise<LoadResult> {
    const request = ++viewRequest.current;
    setLoading(true);
    setError("");
    try {
      const next = await getInventoryView({
        product_search: productSearch || undefined,
        product_page: productPage,
        base_unit: (productUnit || undefined) as BaseUnit | undefined,
        cost_status: (costStatus || undefined) as InventoryCostStatus | undefined,
        movement_search: movementSearch || undefined,
        movement_page: movementPage,
        reason: (reason || undefined) as InventoryMovement["reason"] | undefined,
        type: (movementType || undefined) as InventoryMovement["type"] | undefined,
      });
      if (viewRequest.current !== request) return "stale";
      setView({
        ...next,
        movements: next.movements.map((movement) => ({
          ...movement,
          product_name: movement.product_name_snapshot,
          sku: movement.sku_snapshot,
        })),
      });
      return "success";
    } catch (value: unknown) {
      if (viewRequest.current !== request) return "stale";
      if (isSessionRequired(value)) { onSessionRequired(); return "session"; }
      setError(errorMessage(value, "No se pudo consultar el inventario."));
      return "error";
    } finally {
      if (viewRequest.current === request) setLoading(false);
    }
  }

  async function loadProducts(page = 1, append = false, search = selectorSearch) {
    if (selectorDismissed.current) return false;
    const request = ++productsRequest.current;
    productsRetry.current = { page, append, search };
    setProductsLoading(true);
    setProductsError("");
    setSelectorOpen(true);
    try {
      const next = await searchInventoryProducts(search || undefined, page);
      if (productsRequest.current !== request) return false;
      setProducts((current) => append
        ? [...current, ...next.items.filter((item) => !current.some((product) => product.id === item.id))]
        : next.items);
      setSelectorPage(next.page);
      setSelectorHasMore(next.page * next.page_size < next.total);
      if (!append) setActiveProductIndex(next.items.length ? 0 : -1);
      setSelectorOpen(true);
      return true;
    } catch (value: unknown) {
      if (productsRequest.current !== request) return false;
      if (isSessionRequired(value)) onSessionRequired();
      else setProductsError(errorMessage(value, "No se pudieron cargar los productos."));
      return false;
    } finally {
      if (productsRequest.current === request) setProductsLoading(false);
    }
  }

  useEffect(() => {
    mounted.current = true;
    lifetime.current += 1;
    return () => { mounted.current = false; lifetime.current += 1; viewRequest.current += 1; productsRequest.current += 1; quoteRequest.current += 1; };
  }, []);

  useEffect(() => {
    const timeout = window.setTimeout(() => void loadView(), 250);
    return () => { window.clearTimeout(timeout); viewRequest.current += 1; };
  }, [user.id, productSearch, productUnit, costStatus, productPage, movementSearch, reason, movementType, movementPage]);

  useEffect(() => {
    lifetime.current += 1;
    viewRequest.current += 1;
    productsRequest.current += 1;
    saveInFlight.current = false;
    setSaving(false);
    setView(undefined);
    setProducts([]);
    setSelectedProductId(undefined);
    setMovementOpen(false);
    resetMovementForm();
    setPendingIntent(readPendingManualMovement(user.id));
    setIntentCanBeDiscarded(false); setCleanupRequired(false); setDiscardOpen(false); setDiscardError("");
  }, [user.id]);

  useEffect(() => {
    if (!movementOpen || productId) return;
    const search = selectorSearch;
    const timeout = window.setTimeout(() => void loadProducts(1, false, search), 250);
    return () => { window.clearTimeout(timeout); productsRequest.current += 1; };
  }, [user.id, movementOpen, productId, selectorSearch]);

  useEffect(() => {
    if (!selectorOpen || activeProductIndex < 0) return;
    document.getElementById(`inventory-product-option-${products[activeProductIndex]?.id}`)?.scrollIntoView?.({ block: "nearest" });
  }, [selectorOpen, activeProductIndex, products]);

  function updateProductFilter(setter: (value: string) => void, value: string) { setProductPage(1); setter(value); }
  function updateMovementFilter(setter: (value: string) => void, value: string) { setMovementPage(1); setter(value); }

  function selectTab(next: Tab) {
    setTab(next);
    window.requestAnimationFrame(() => document.getElementById(`inventory-${next}-tab`)?.focus());
  }

  function handleTabKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    let next: Tab | undefined;
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") next = tab === "products" ? "movements" : "products";
    else if (event.key === "Home") next = "products";
    else if (event.key === "End") next = "movements";
    if (!next) return;
    event.preventDefault();
    selectTab(next);
  }

  function showDetail(id: number, focusId: string) {
    returnFocusId.current = focusId;
    setSelectedProductId(id);
  }

  function backFromDetail() {
    setSelectedProductId(undefined);
    window.requestAnimationFrame(() => {
      document.querySelector<HTMLElement>(`[data-inventory-product="${returnFocusId.current}"]`)?.focus();
    });
  }

  function resetMovementForm() {
    setProductId(""); setPresentationId(""); setOperation("purchase"); setQuantity(""); setUnitPrice("");
    setSupplier(""); setDocumentReference(""); setNote(""); setMovementError(""); setQuantityError(""); setUnitPriceError("");
    setSelectorSearch(""); setProducts([]); setSelectorPage(1); setSelectorHasMore(false); setSelectorOpen(false); setActiveProductIndex(-1); setProductsError("");
    productsRetry.current = { page: 1, append: false, search: "" };
    selectorDismissed.current = false;
    quoteRequest.current += 1; setQuote(undefined); setQuoteLoading(false); setDraftOperationToken(undefined);
  }

  function invalidateQuote() { quoteRequest.current += 1; setQuote(undefined); setQuoteLoading(false); }

  function openMovement() { resetMovementForm(); setMovementOpen(true); }
  function closeMovement() { if (saveInFlight.current) return; setMovementOpen(false); resetMovementForm(); }

  async function submitIntent(intent: PendingManualMovementV4) {
    if (saveInFlight.current || intent.userId !== user.id) return;
    const requestLifetime = lifetime.current;
    saveInFlight.current = true; setSaving(true); setMovementError(""); setQuantityError(""); setUnitPriceError(""); setIntentCanBeDiscarded(false);
    let result: InventoryMovement | undefined;
    try {
      result = await submitMovementOnce(intent);
      if (!mounted.current || lifetime.current !== requestLifetime) return;
      const cleanup = clearPendingManualMovement(intent.userId, intent.operationToken);
      if (cleanup !== "cleared" && cleanup !== "absent") {
        const message = "El movimiento fue confirmado, pero no se pudo limpiar la recuperacion local. Reintenta la limpieza de forma segura.";
        setCleanupRequired(true); setMovementError(message); onNotice({ tone: "info", message }); return;
      }
      setPendingIntent(null);
    } catch (value: unknown) {
      if (!mounted.current || lifetime.current !== requestLifetime) return;
      if (isSessionRequired(value)) { setMovementOpen(false); onSessionRequired(); }
      else {
        const message = errorMessage(value, "No se pudo confirmar el movimiento. El registro pendiente se conservo para reintentar.");
        if (isAppError(value) && ["QUOTE_CHANGED", "INVENTORY_QUOTE_REQUIRED"].includes(value.code)) {
          setDraftOperationToken(intent.operationToken);
          invalidateQuote();
          try {
            const next = await quoteInventoryMovement({
              product_id: intent.productId,
              presentation_id: intent.presentationId,
              reason: intent.reason,
              type: intent.type,
              quantity: intent.quantity,
              unit_price: intent.unitPrice,
              supplier: intent.supplier,
              document_reference: intent.documentReference,
              note: intent.note,
            });
            if (mounted.current && lifetime.current === requestLifetime) {
              setQuote(next);
              setMovementError(value.code === "QUOTE_CHANGED" ? "La cotizacion cambio. Revisa el nuevo importe y confirma nuevamente." : "Revisa el importe actualizado y confirma el movimiento.");
            }
          } catch (quoteError: unknown) {
            if (mounted.current && lifetime.current === requestLifetime) {
              setMovementError(errorMessage(quoteError, "La cotizacion cambio. Calcula nuevamente el importe antes de confirmar."));
            }
          }
          return;
        }
        if (isAppError(value) && value.field === "quantity") { setQuantityError(message); window.requestAnimationFrame(() => quantityRef.current?.focus()); }
        else if (isAppError(value) && value.field === "unit_price") { setUnitPriceError(message); window.requestAnimationFrame(() => unitPriceRef.current?.focus()); }
        else setMovementError(message);
        if (isAppError(value) && value.category === "validation") {
          const cleanup = clearPendingManualMovement(intent.userId, intent.operationToken);
          if (cleanup === "cleared" || cleanup === "absent") {
            setPendingIntent(null); setDraftOperationToken(undefined); setQuote(undefined);
            setMovementError(message); setIntentCanBeDiscarded(false);
          } else {
            setMovementError(`${message} El registro pendiente se conservo porque cambio en este dispositivo.`);
          }
        } else setIntentCanBeDiscarded(isAppError(value) && value.category === "conflict");
      }
    } finally {
      if (lifetime.current === requestLifetime) {
        saveInFlight.current = false;
        if (mounted.current) setSaving(false);
      }
    }
    if (!result || !mounted.current) return;
    setMovementOpen(false); resetMovementForm();
    const confirmed = result.movement_cost_cents == null ? "" : ` Costo del movimiento: ${formatMoney(result.movement_cost_cents)}.`;
    onNotice({ tone: "success", message: `Movimiento de inventario registrado.${confirmed}` });
    if (await loadView() === "error") onNotice({ tone: "info", message: "El movimiento se guardo, pero no se pudo recargar el inventario; usa Reintentar." });
  }

  async function recoverIntent(intent: PendingManualMovement) {
    const requestLifetime = lifetime.current;
    const request = ++productsRequest.current;
    setProductId(String(intent.productId)); setPresentationId(intent.version !== 1 && intent.presentationId ? String(intent.presentationId) : "");
    setOperation(operationFor(intent)); setQuantity(intent.quantity);
    setUnitPrice(intent.version === 2 || intent.version === 4 ? intent.unitPrice ?? "" : "");
    setSupplier(intent.version !== 1 ? intent.supplier ?? "" : "");
    setDocumentReference(intent.version !== 1 ? intent.documentReference ?? "" : ""); setNote(intent.version !== 1 ? intent.note ?? "" : "");
    setQuote(undefined); setMovementError(""); setQuantityError(""); setUnitPriceError(""); setMovementOpen(true); setProductsLoading(true);
    try {
      let page = 1;
      let recoveredProduct: InventoryMovementProduct | undefined;
      while (!recoveredProduct) {
        const result = await searchInventoryProducts(undefined, page);
        if (lifetime.current !== requestLifetime || productsRequest.current !== request) return;
        recoveredProduct = result.items.find((product) => product.id === intent.productId);
        if (recoveredProduct || result.page * result.page_size >= result.total) break;
        page += 1;
      }
      if (!recoveredProduct) {
        setMovementError("El producto del movimiento pendiente ya no esta disponible. No se intento confirmar ni recotizar.");
        return;
      }
      const recoveredPresentationId = intent.version === 1
        ? recoveredProduct.base_unit === "piece"
          ? recoveredProduct.presentations.find((presentation) => presentation.is_active && presentation.mode === "fixed" && presentation.base_quantity === 1)?.id ?? null
          : null
        : intent.version === 2 && intent.presentationId !== null
          ? recoveredProduct.presentations.some((presentation) =>
              presentation.id === intent.presentationId && presentation.is_active &&
              (recoveredProduct.base_unit === "piece" || presentation.mode === "measured"),
            ) ? intent.presentationId : null
          : intent.presentationId;
      setProducts([recoveredProduct]);
      setPresentationId(recoveredPresentationId === null ? "" : String(recoveredPresentationId));
      if (intent.version === 4) {
        await submitIntent(intent);
        return;
      }
      const recoveredUnitPrice = intent.version === 2 ? intent.unitPrice : null;
      if (intent.type === "entrada" && !recoveredUnitPrice) {
        setDraftOperationToken(intent.operationToken);
        setMovementError("Revisa el contexto y captura nuevamente el precio de compra antes de cotizar.");
        window.requestAnimationFrame(() => unitPriceRef.current?.focus());
        return;
      }
      const next = await quoteInventoryMovement({
        product_id: intent.productId,
        presentation_id: intent.type === "valoracion" ? null : recoveredPresentationId,
        reason: intent.reason,
        type: intent.type,
        quantity: intent.quantity,
        unit_price: intent.type === "salida" ? null : recoveredUnitPrice,
        supplier: intent.version === 1 ? null : intent.supplier,
        document_reference: intent.version === 1 ? null : intent.documentReference,
        note: intent.version === 1 ? null : intent.note,
      });
      if (lifetime.current !== requestLifetime || productsRequest.current !== request) return;
      setDraftOperationToken(intent.operationToken);
      setQuote(next);
      setMovementError("El movimiento pendiente se recupero como contexto. Revisa la cotizacion y confirma nuevamente.");
    } catch (value: unknown) {
      if (lifetime.current !== requestLifetime || productsRequest.current !== request) return;
      if (isSessionRequired(value)) { setMovementOpen(false); onSessionRequired(); }
      else setMovementError(errorMessage(value, "No se pudo recuperar el producto y sus formas de captura. Intenta de nuevo."));
    } finally {
      if (lifetime.current === requestLifetime && productsRequest.current === request) setProductsLoading(false);
    }
  }

  async function retryCleanup() {
    if (!pendingIntent) return;
    const cleanup = clearPendingManualMovement(pendingIntent.userId, pendingIntent.operationToken);
    if (cleanup !== "cleared" && cleanup !== "absent") { setMovementError("El movimiento sigue confirmado, pero no se pudo limpiar la recuperacion local."); return; }
    setPendingIntent(null); setCleanupRequired(false); setMovementOpen(false); setMovementError("");
    onNotice({ tone: "success", message: "Recuperacion local limpiada; el movimiento ya estaba confirmado." }); await loadView();
  }

  async function saveMovement(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saveInFlight.current) return;
    if (cleanupRequired) { await retryCleanup(); return; }
    if (pendingIntent?.version === 4 && !draftOperationToken) { await submitIntent(pendingIntent); return; }
    if (!productId) { setMovementError("Selecciona un producto."); return; }
    const kind = operations[operation];
    if (!quote) {
      const request = ++quoteRequest.current;
      setQuoteLoading(true); setMovementError(""); setQuantityError(""); setUnitPriceError("");
      try {
        const next = await quoteInventoryMovement({
          product_id: Number(productId), presentation_id: operation === "valuation" || !presentationId ? null : Number(presentationId),
          ...kind, quantity: operation === "valuation" ? "0" : quantity,
          unit_price: operation === "adjustment-out" ? null : unitPrice, supplier: supplier || null,
          document_reference: documentReference || null, note: note || null,
        });
        if (quoteRequest.current === request) setQuote(next);
      } catch (value: unknown) {
        if (quoteRequest.current !== request) return;
        if (isSessionRequired(value)) { setMovementOpen(false); onSessionRequired(); }
        else {
          const message = errorMessage(value, "No se pudo calcular el movimiento.");
          if (isAppError(value) && value.field === "quantity") setQuantityError(message);
          else if (isAppError(value) && value.field === "unit_price") { setUnitPriceError(message); window.requestAnimationFrame(() => unitPriceRef.current?.focus()); }
          else setMovementError(message);
        }
      } finally { if (quoteRequest.current === request) setQuoteLoading(false); }
      return;
    }
    const intent: PendingManualMovementV4 = {
      version: 4, userId: user.id, operationToken: draftOperationToken ?? crypto.randomUUID(), productId: Number(productId),
      presentationId: operation === "valuation" || !presentationId ? null : Number(presentationId), ...kind,
      quantity: operation === "valuation" ? "0" : quantity,
      unitPrice: operation === "adjustment-out" ? null : unitPrice,
      supplier: supplier || null, documentReference: documentReference || null, note: note || null,
      quoteFingerprint: quote.quote_fingerprint,
    };
    try {
      if (pendingIntent && draftOperationToken) {
        if (!replacePendingManualMovement(pendingIntent, intent)) { setMovementError("El borrador guardado cambio. Vuelve a abrirlo antes de confirmar."); return; }
      } else savePendingManualMovement(intent);
    } catch { setMovementError("No se pudo guardar la operacion pendiente. Intenta de nuevo."); return; }
    setPendingIntent(intent); await submitIntent(intent);
  }

  async function discardIntent() {
    if (!pendingIntent) return;
    const cleanup = clearPendingManualMovement(pendingIntent.userId, pendingIntent.operationToken);
    if (cleanup !== "cleared" && cleanup !== "absent") { setDiscardError("No se pudo eliminar la recuperacion local. El movimiento pendiente se conservo."); return; }
    setPendingIntent(null); setIntentCanBeDiscarded(false); setDiscardOpen(false); setDiscardError(""); setMovementOpen(false); setMovementError("");
    await loadView();
  }

  const selectedProduct = products.find((product) => product.id === Number(productId));
  const activePresentations = selectedProduct?.presentations.filter((presentation) =>
    presentation.is_active && (selectedProduct.base_unit === "piece" || presentation.mode === "measured"),
  ) ?? [];
  const needsCost = operation !== "adjustment-out";
  const isValuation = operation === "valuation";
  const pageSize = view?.page_size ?? 20;
  const selectedPresentation = activePresentations.find((presentation) => presentation.id === Number(presentationId));
  const selectedIsDirectCapture = Boolean(selectedProduct) && (!selectedPresentation || selectedPresentation.mode === "measured");
  const directCaptureIsDuplicated = selectedProduct?.base_unit === "piece" && activePresentations.some((presentation) => presentation.mode === "fixed" && presentation.base_quantity === 1);
  const captureOptions: Array<{ key: string; presentation: InventoryMovementPresentation | null; title: string; description: string }> = [];
  if (selectedProduct && !directCaptureIsDuplicated) {
    captureOptions.push({
      key: "base",
      presentation: null,
      title: directCaptureTitles[selectedProduct.base_unit],
      description: directCaptureDescriptions[selectedProduct.base_unit],
    });
  }
  let includedEquivalentBase = false;
  for (const presentation of activePresentations) {
    const equivalentBase = selectedProduct?.base_unit === "piece" && presentation.mode === "fixed" && presentation.base_quantity === 1;
    if (equivalentBase && includedEquivalentBase) continue;
    if (equivalentBase) includedEquivalentBase = true;
    captureOptions.push({
      key: String(presentation.id),
      presentation,
      title: equivalentBase ? directCaptureTitles[selectedProduct!.base_unit] : presentation.name,
      description: equivalentBase
        ? directCaptureDescriptions[selectedProduct!.base_unit]
        : presentation.mode === "fixed"
          ? `Cada ${presentation.name} agrega o retira ${formatBaseQuantity(presentation.base_quantity, selectedProduct!.base_unit)}.`
          : `${directCaptureDescriptions[selectedProduct!.base_unit]} El precio corresponde a ${formatBaseQuantity(presentation.base_quantity, selectedProduct!.base_unit)}.`,
    });
  }

  function selectProduct(product: InventoryMovementProduct) {
    const equivalentBase = product.base_unit === "piece"
      ? product.presentations.find((presentation) => presentation.is_active && presentation.mode === "fixed" && presentation.base_quantity === 1)
      : undefined;
    setProductId(String(product.id)); setPresentationId(equivalentBase ? String(equivalentBase.id) : ""); setSelectorOpen(false); setMovementError(""); invalidateQuote();
  }

  function changeProduct() {
    selectorDismissed.current = false;
    setProductId(""); setPresentationId(""); setSelectorOpen(true); invalidateQuote();
    window.requestAnimationFrame(() => selectorInputRef.current?.focus());
  }

  function handleProductKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Escape") {
      event.preventDefault(); event.stopPropagation(); selectorDismissed.current = true;
      productsRequest.current += 1; setProductsLoading(false); setSelectorOpen(false); return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault(); selectorDismissed.current = false; setSelectorOpen(true);
      setActiveProductIndex((current) => {
        if (!products.length) return -1;
        if (event.key === "ArrowDown") return current < products.length - 1 ? current + 1 : 0;
        return current > 0 ? current - 1 : products.length - 1;
      });
      return;
    }
    if (event.key === "Enter" && selectorOpen && !productsLoading && activeProductIndex >= 0) {
      event.preventDefault(); selectProduct(products[activeProductIndex]);
    }
  }
  function selectCaptureOption(index: number, focus = false) {
    const option = captureOptions[index];
    if (!option) return;
    setPresentationId(option.presentation ? String(option.presentation.id) : "");
    invalidateQuote();
    if (focus) window.requestAnimationFrame(() => document.getElementById(`inventory-capture-option-${option.key}`)?.focus());
  }
  function handleCaptureKeyDown(event: KeyboardEvent<HTMLButtonElement>, index: number) {
    let next: number | undefined;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") next = (index + 1) % captureOptions.length;
    else if (event.key === "ArrowLeft" || event.key === "ArrowUp") next = (index - 1 + captureOptions.length) % captureOptions.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = captureOptions.length - 1;
    if (next === undefined) return;
    event.preventDefault();
    selectCaptureOption(next, true);
  }
  const quantityLabel = selectedIsDirectCapture && selectedProduct
    ? directQuantityLabels[selectedProduct.base_unit]
    : selectedPresentation
      ? selectedPresentation.base_quantity === 1 && selectedProduct?.base_unit === "piece" ? "Cantidad de piezas" : "Cantidad de paquetes"
      : `Cantidad en ${selectedProduct ? unitLabels[selectedProduct.base_unit].toLowerCase() : "unidad base"}`;
  const purchasePriceLabel = selectedPresentation?.mode === "fixed"
    ? `Precio de compra por ${selectedPresentation.name}`
    : selectedPresentation?.mode === "measured" && selectedProduct
      ? `Precio de compra por ${formatBaseQuantity(selectedPresentation.base_quantity, selectedProduct.base_unit)}`
      : `Precio de compra por ${selectedProduct ? unitLabels[selectedProduct.base_unit].toLowerCase().replace(/s$/, "") : "unidad base"}`;
  return (
    <main id="main-content" tabIndex={-1} className="p-4 outline-none sm:p-6 min-[900px]:p-8">
      {selectedProductId ? <ProductInventoryDetail productId={selectedProductId} onBack={backFromDetail} onSessionRequired={onSessionRequired} /> : <>
        {pendingIntent && <section className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-card border border-warning/30 bg-warning-soft p-4" role="status"><div><strong className="text-sm text-ink">{cleanupRequired ? "Movimiento confirmado con recuperacion pendiente" : "Movimiento pendiente"}</strong><p className="mt-1 text-xs text-muted">{cleanupRequired ? "El movimiento no se repetira; solo falta retirar el registro local." : pendingIntent.version === 4 ? "Puedes revisar los datos o reintentar el envio de forma segura." : "Se recuperara solo como contexto y debera revisarse y cotizarse de nuevo."}</p></div><div className="flex flex-wrap gap-2">{cleanupRequired ? <Button disabled={saving} onClick={() => void retryCleanup()}>Reintentar limpieza</Button> : <Button disabled={saving || productsLoading} onClick={() => void recoverIntent(pendingIntent)}>{pendingIntent.version === 4 ? "Reintentar envio" : "Revisar y completar"}</Button>}{intentCanBeDiscarded && !cleanupRequired && <Button variant="ghost" disabled={saving} onClick={() => { setMovementOpen(false); setDiscardOpen(true); }}>Descartar pendiente</Button>}</div></section>}

        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3" aria-busy={loading}>
          <StatCard label="Productos" value={view?.stats.product_count ?? "-"} />
          <StatCard label="Existencia en piezas" value={view ? formatBaseQuantity(view.stats.piece_stock, "piece") : "-"} />
          <StatCard label="Existencia por peso" value={view ? formatBaseQuantity(view.stats.gram_stock, "gram") : "-"} />
          <StatCard label="Existencia por volumen" value={view ? formatBaseQuantity(view.stats.milliliter_stock, "milliliter") : "-"} />
          <StatCard label="Existencia por longitud" value={view ? formatBaseQuantity(view.stats.centimeter_stock, "centimeter") : "-"} />
          <StatCard label="Valor total a costo" value={view ? formatMoney(view.stats.inventory_value_cents) : "-"} detail="No incluye productos con costo desconocido." />
          <StatCard label="Costo desconocido" value={view?.stats.products_without_cost ?? "-"} detail="Productos fuera del valor total." />
        </div>

        <section className="mt-5 overflow-hidden rounded-card border border-line bg-surface">
          <div className="flex flex-wrap items-end justify-between gap-3 border-b border-line p-4 sm:p-5"><div><h1 className="font-heading text-2xl font-bold">Inventario</h1><p className="mt-1 text-xs text-muted">Existencias y costos acumulados por producto.</p></div><Button variant="primary" onClick={openMovement} disabled={Boolean(pendingIntent)}>Registrar movimiento</Button></div>
          <div className="border-b border-line px-4 pt-3" role="tablist" aria-label="Secciones de inventario"><button id="inventory-products-tab" role="tab" aria-selected={tab === "products"} aria-controls="inventory-products-panel" tabIndex={tab === "products" ? 0 : -1} className={`mr-2 border-b-2 px-4 py-3 text-sm font-bold ${tab === "products" ? "border-brand text-brand" : "border-transparent text-muted"}`} onClick={() => setTab("products")} onKeyDown={handleTabKeyDown}>Productos</button><button id="inventory-movements-tab" role="tab" aria-selected={tab === "movements"} aria-controls="inventory-movements-panel" tabIndex={tab === "movements" ? 0 : -1} className={`border-b-2 px-4 py-3 text-sm font-bold ${tab === "movements" ? "border-brand text-brand" : "border-transparent text-muted"}`} onClick={() => setTab("movements")} onKeyDown={handleTabKeyDown}>Movimientos</button></div>

          {error && <div className="m-4 flex flex-wrap items-center justify-between gap-3 rounded-control border border-danger/25 bg-danger-soft px-4 py-3 text-sm text-danger" role="alert"><span>{error}</span><Button size="sm" onClick={() => void loadView()} disabled={loading}>Reintentar</Button></div>}
          {error && view && <p className="mx-4 mb-3 text-xs font-semibold text-muted">Se muestran los resultados anteriores; no corresponden a los filtros actuales.</p>}
          {loading && view && <p className="mx-4 mb-3 text-xs font-semibold text-muted">Actualizando; se muestran temporalmente los resultados anteriores.</p>}

          <div id="inventory-products-panel" role="tabpanel" aria-labelledby="inventory-products-tab" hidden={tab !== "products"}>
            <div className="grid gap-3 border-b border-line bg-surface-soft/60 p-4 sm:grid-cols-3"><Field label="Buscar producto" value={productSearch} onChange={(event) => updateProductFilter(setProductSearch, event.target.value)} placeholder="Nombre o SKU" /><label className="grid gap-2 text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">Unidad base<select className={selectClass} value={productUnit} onChange={(event) => updateProductFilter(setProductUnit, event.target.value)}><option value="">Todas</option><option value="piece">Pieza</option><option value="gram">Gramo</option><option value="milliliter">Mililitro</option><option value="centimeter">Centimetro</option></select></label><label className="grid gap-2 text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">Estado de costo<select className={selectClass} value={costStatus} onChange={(event) => updateProductFilter(setCostStatus, event.target.value)}><option value="">Todos</option><option value="known">Con costo conocido</option><option value="unknown">Costo desconocido</option></select></label></div>
            <div className="relative overflow-x-auto focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand" role="region" aria-label="Productos de inventario desplazables" aria-busy={loading} tabIndex={0}>{!view && loading ? <div className="grid min-h-64 place-items-center"><Spinner className="size-6 text-brand" /></div> : view?.products.length ? <table className="w-full min-w-[760px] text-left text-sm"><caption className="sr-only">Productos en inventario</caption><thead><tr className="text-[0.625rem] tracking-wider text-muted uppercase"><th className="border-b border-line px-5 py-3">Producto</th><th className="border-b border-line px-5 py-3">Unidad</th><th className="border-b border-line px-5 py-3 text-right">Stock</th><th className="border-b border-line px-5 py-3 text-right">Costo promedio</th><th className="border-b border-line px-5 py-3 text-right">Valor a costo</th><th className="border-b border-line px-5 py-3"><span className="sr-only">Acciones</span></th></tr></thead><tbody>{view.products.map((product) => <tr key={product.id} className="hover:bg-surface-soft/60"><td className="border-b border-line px-5 py-4"><strong className="block">{product.name}</strong><span className="font-mono text-xs text-muted">{product.sku}</span></td><td className="border-b border-line px-5 py-4">{unitLabels[product.base_unit]}</td><td className="border-b border-line px-5 py-4 text-right font-bold">{formatBaseQuantity(product.stock, product.base_unit)}</td><td className="border-b border-line px-5 py-4 text-right">{product.average_cost_cents === null ? <span className="text-warning">Desconocido</span> : formatMoney(product.average_cost_cents)}</td><td className="border-b border-line px-5 py-4 text-right">{product.inventory_cost_cents === null ? <span className="text-warning">Desconocido</span> : formatMoney(product.inventory_cost_cents)}</td><td className="border-b border-line px-5 py-4 text-right"><Button size="sm" aria-label={`Ver inventario de ${product.name}`} data-inventory-product={product.id} onClick={() => showDetail(product.id, String(product.id))}>Ver inventario</Button></td></tr>)}</tbody></table> : !loading && !error ? <EmptyState title="No hay productos con estos filtros" description="Cambia la busqueda o los filtros para ampliar los resultados." icon="□" /> : null}</div>
            {view && <Pagination label="Paginacion de productos" page={productPage} total={view.product_total} pageSize={pageSize} onPage={setProductPage} />}
          </div>

          <div id="inventory-movements-panel" role="tabpanel" aria-labelledby="inventory-movements-tab" hidden={tab !== "movements"}>
            <div className="grid gap-3 border-b border-line bg-surface-soft/60 p-4 sm:grid-cols-3"><Field label="Buscar movimiento" value={movementSearch} onChange={(event) => updateMovementFilter(setMovementSearch, event.target.value)} placeholder="Producto, proveedor, documento o ticket" /><label className="grid gap-2 text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">Motivo<select className={selectClass} value={reason} onChange={(event) => updateMovementFilter(setReason, event.target.value)}><option value="">Todos</option><option value="compra">Compra</option><option value="venta">Venta</option><option value="ajuste">Ajuste</option><option value="valoracion">Valoracion historica</option></select></label><label className="grid gap-2 text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">Tipo<select className={selectClass} value={movementType} onChange={(event) => updateMovementFilter(setMovementType, event.target.value)}><option value="">Todos</option><option value="entrada">Entrada</option><option value="salida">Salida</option><option value="valoracion">Valoracion historica</option></select></label></div>
            <div className="relative overflow-x-auto focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand" role="region" aria-label="Movimientos de inventario desplazables" aria-busy={loading} tabIndex={0}>{!view && loading ? <div className="grid min-h-64 place-items-center"><Spinner className="size-6 text-brand" /></div> : view?.movements.length ? <table className="w-full min-w-[850px] text-left text-sm"><caption className="sr-only">Movimientos historicos de inventario</caption><thead><tr className="text-[0.625rem] tracking-wider text-muted uppercase"><th className="border-b border-line px-5 py-3">Producto</th><th className="border-b border-line px-5 py-3">Movimiento</th><th className="border-b border-line px-5 py-3 text-right">Cantidad base</th><th className="border-b border-line px-5 py-3 text-right">Costo del movimiento</th><th className="border-b border-line px-5 py-3">Referencia</th><th className="border-b border-line px-5 py-3">Fecha</th></tr></thead><tbody>{view.movements.map((movement) => <tr key={movement.id} className="hover:bg-surface-soft/60"><td className="border-b border-line px-5 py-4"><strong className="block">{movement.product_name}</strong><span className="text-xs text-muted">{movement.sku}</span></td><td className="border-b border-line px-5 py-4"><strong>{movement.type === "entrada" ? "Entrada" : movement.type === "salida" ? "Salida" : "Valoracion"}</strong><span className="ml-2 text-xs text-muted">{reasonLabels[movement.reason]}</span></td><td className="border-b border-line px-5 py-4 text-right font-bold">{movement.type === "valoracion" ? "Sin cambio" : movement.base_quantity !== null && movement.base_unit ? formatBaseQuantity(movement.base_quantity, movement.base_unit) : movement.quantity}</td><td className="border-b border-line px-5 py-4 text-right">{movement.movement_cost_cents == null ? <span className="text-warning">Desconocido</span> : formatMoney(movement.movement_cost_cents)}</td><td className="border-b border-line px-5 py-4">{movement.ticket_reference ?? movement.document_reference ?? "-"}</td><td className="border-b border-line px-5 py-4 text-xs whitespace-nowrap text-muted">{formatDateTime(movement.created_at)}</td></tr>)}</tbody></table> : !loading && !error ? <EmptyState title="No hay movimientos con estos filtros" description="Cambia la busqueda o los filtros para ampliar los resultados." icon="↕" /> : null}</div>
            {view && <Pagination label="Paginacion de movimientos" page={movementPage} total={view.movement_total} pageSize={pageSize} onPage={setMovementPage} />}
          </div>
        </section>
      </>}

      <Modal open={movementOpen} onClose={closeMovement} closeDisabled={saving || quoteLoading} title="Registrar movimiento" description="Revisa la cantidad aplicada y el importe antes de confirmar." size="md" footer={<>{intentCanBeDiscarded && <Button variant="ghost" onClick={() => { setMovementOpen(false); setDiscardOpen(true); }} disabled={saving}>Descartar pendiente</Button>}<Button onClick={closeMovement} disabled={saving || quoteLoading}>Cancelar</Button><Button form="inventory-movement-form" type="submit" variant="primary" loading={saving || quoteLoading}>{cleanupRequired ? "Reintentar limpieza" : quote ? "Confirmar movimiento" : "Calcular importe"}</Button></>}>
        <form id="inventory-movement-form" className="grid gap-4" noValidate onSubmit={(event) => void saveMovement(event)}>
          {movementError && <div className="rounded-control border border-danger/25 bg-danger-soft px-4 py-3 text-sm text-danger" role="alert">{movementError}</div>}
          {!selectedProduct ? <div className="relative grid gap-2">
            <label htmlFor="inventory-product-combobox" className="text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">Buscar producto</label>
            <input
              ref={selectorInputRef}
              data-autofocus
              id="inventory-product-combobox"
              role="combobox"
              aria-autocomplete="list"
              aria-expanded={selectorOpen}
              aria-controls="inventory-product-listbox"
              aria-activedescendant={selectorOpen && activeProductIndex >= 0 ? `inventory-product-option-${products[activeProductIndex]?.id}` : undefined}
              autoComplete="off"
              className={selectClass}
              value={selectorSearch}
              onFocus={() => { if (!selectorDismissed.current) setSelectorOpen(true); }}
              onClick={() => { selectorDismissed.current = false; setSelectorOpen(true); }}
              onChange={(event) => { selectorDismissed.current = false; setSelectorSearch(event.target.value); setSelectorPage(1); setSelectorHasMore(false); setProductsError(""); setProductsLoading(true); setSelectorOpen(true); setActiveProductIndex(-1); }}
              onKeyDown={handleProductKeyDown}
              placeholder="Nombre o SKU"
              disabled={saving}
            />
            {selectorOpen && <div id="inventory-product-listbox" role="listbox" aria-label="Productos" className="max-h-72 overflow-y-auto rounded-control border border-line-strong bg-surface shadow-lg">
              {products.map((product, index) => <button
                key={product.id}
                id={`inventory-product-option-${product.id}`}
                type="button"
                role="option"
                tabIndex={-1}
                aria-selected={index === activeProductIndex}
                aria-disabled={productsLoading}
                className={`block w-full border-b border-line px-4 py-3 text-left outline-none last:border-0 hover:bg-surface-soft focus:bg-surface-soft ${index === activeProductIndex ? "bg-brand/5" : ""}`}
                onMouseEnter={() => setActiveProductIndex(index)}
                onClick={() => selectProduct(product)}
                disabled={productsLoading}
              ><strong className="block text-sm text-ink">{product.name}</strong><span className="mt-1 block text-xs text-muted">SKU {product.sku} · Stock {formatBaseQuantity(product.stock, product.base_unit)}</span></button>)}
              {productsLoading && <p className="px-4 py-3 text-sm text-muted" role="status">{products.length ? "Actualizando resultados anteriores..." : "Buscando productos..."}</p>}
              {!productsLoading && productsError && <div className="flex items-center justify-between gap-3 px-4 py-3" role="alert"><span className="text-sm text-danger">{productsError}</span><Button size="sm" onClick={() => { const retry = productsRetry.current; void loadProducts(retry.page, retry.append, retry.search); }}>Reintentar</Button></div>}
              {!productsLoading && !productsError && !products.length && <p className="px-4 py-5 text-sm text-muted" role="status">No se encontraron productos.</p>}
              {!productsError && selectorHasMore && <div role="none" className="p-2"><Button size="sm" onClick={() => { void loadProducts(selectorPage + 1, true).then(() => { window.requestAnimationFrame(() => selectorInputRef.current?.focus()); }); }} disabled={productsLoading}>Cargar mas</Button></div>}
            </div>}
          </div> : <section className="rounded-control border border-line bg-surface-soft p-4" aria-label="Producto seleccionado">
            <div className="flex items-start justify-between gap-3"><div><strong className="block text-ink">{selectedProduct.name}</strong><span className="mt-1 block font-mono text-xs text-muted">{selectedProduct.sku}</span><span className="mt-2 block text-sm text-muted">Stock actual: <b className="text-ink">{formatBaseQuantity(selectedProduct.stock, selectedProduct.base_unit)}</b></span></div><Button size="sm" onClick={changeProduct} disabled={saving}>Cambiar producto</Button></div>
          </section>}
          {!isValuation && selectedProduct && <fieldset className="grid gap-2"><legend className="text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">Como estas contando esta cantidad?</legend><div className="grid gap-2 sm:grid-cols-2" role="radiogroup">{captureOptions.map((option, index) => {
            const value = option.presentation ? String(option.presentation.id) : "";
            const selected = presentationId === value;
            return <button key={option.key} id={`inventory-capture-option-${option.key}`} type="button" role="radio" aria-checked={selected} tabIndex={selected || (!presentationId && index === 0) ? 0 : -1} className={`rounded-control border p-3 text-left outline-none transition focus:ring-3 focus:ring-brand/20 ${selected ? "border-brand bg-brand/5" : "border-line-strong bg-surface hover:border-brand/50"}`} onClick={() => selectCaptureOption(index)} onKeyDown={(event) => handleCaptureKeyDown(event, index)} disabled={saving}><strong className="block text-sm text-ink">{option.title}</strong><span className="mt-1 block text-xs leading-5 text-muted">{option.description}</span></button>;
          })}</div></fieldset>}
          <label className="grid gap-2 text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">Operacion <span className="sr-only">obligatorio</span><select className={selectClass} value={operation} onChange={(event) => { setOperation(event.target.value as Operation); invalidateQuote(); setQuantityError(""); setUnitPriceError(""); }} required disabled={saving}><option value="purchase">Compra</option><option value="adjustment-in">Ajuste de entrada</option><option value="adjustment-out">Ajuste de salida</option><option value="valuation">Establecer costo acumulado</option></select></label>
          {!isValuation && <Field ref={quantityRef} label={quantityLabel} value={quantity} onChange={(event) => { setQuantity(event.target.value); invalidateQuote(); setQuantityError(""); }} inputMode="numeric" pattern="[0-9]+" required disabled={saving} hint={selectedIsDirectCapture && selectedProduct?.base_unit !== "piece" ? "Captura directa en la unidad indicada." : "Indica cuantas piezas o paquetes registras."} error={quantityError || undefined} />}
          {needsCost && <Field ref={unitPriceRef} label={isValuation ? "Nuevo costo acumulado del inventario" : purchasePriceLabel} value={unitPrice} onChange={(event) => { setUnitPrice(event.target.value); invalidateQuote(); setUnitPriceError(""); }} inputMode="decimal" required disabled={saving} placeholder="0.00" hint={isValuation ? "Cambia el costo acumulado sin modificar el stock." : "Precio de compra, no precio al cliente."} error={unitPriceError || undefined} />}
          {quote && <div className="grid gap-1 rounded-control border border-brand/25 bg-brand/5 px-4 py-3 text-sm" role="status"><strong className="mb-1 block text-ink">Resumen por confirmar</strong><span className="text-muted">Producto: <b className="text-ink">{quote.product_name}</b> <span className="font-mono">({quote.product_sku})</span></span>{quote.type !== "valoracion" && <><span className="text-muted">Forma de captura: <b className="text-ink">{quote.capture_name}</b></span><span className="text-muted">Cantidad capturada: <b className="text-ink">{quote.entered_quantity_description}</b></span><span className="text-muted">Movimiento al inventario: <b className="text-ink">{quote.base_quantity_description}</b></span></>}{quote.unit_price_cents !== null && <span className="text-muted">{quote.type === "salida" ? "Costo promedio aplicado" : quote.type === "valoracion" ? "Nuevo costo acumulado" : "Precio de compra"}: <b className="text-ink">{formatMoney(quote.unit_price_cents)} {quote.unit_price_description}</b></span>}<span className="text-muted">Costo del movimiento: <b className="text-ink">{quote.total_cost_cents === null ? "Desconocido" : formatMoney(quote.total_cost_cents)}</b></span></div>}
          <div className="grid gap-4 sm:grid-cols-2"><Field label="Proveedor" value={supplier} onChange={(event) => { setSupplier(event.target.value); invalidateQuote(); }} disabled={saving} maxLength={200} /><Field label="Factura o referencia" value={documentReference} onChange={(event) => { setDocumentReference(event.target.value); invalidateQuote(); }} disabled={saving} maxLength={200} /></div>
          <label className="grid gap-2 text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">Nota<textarea className="min-h-24 w-full rounded-control border border-line-strong bg-surface px-3 py-2 text-sm font-normal tracking-normal text-ink normal-case outline-none focus:border-brand focus:ring-3 focus:ring-brand/12 disabled:bg-surface-soft" value={note} onChange={(event) => { setNote(event.target.value); invalidateQuote(); }} disabled={saving} maxLength={1000} /></label>
        </form>
      </Modal>
      <Modal open={discardOpen} onClose={() => setDiscardOpen(false)} title="Descartar movimiento pendiente" description="Descarta solo si revisaste el error definitivo. El inventario se recargara antes de permitir otra operacion." size="sm" footer={<><Button onClick={() => setDiscardOpen(false)}>Conservar</Button><Button variant="danger" onClick={() => void discardIntent()}>Descartar y recargar</Button></>}>
        {discardError && <div className="mb-3 rounded-control border border-danger/25 bg-danger-soft px-4 py-3 text-sm text-danger" role="alert">{discardError}</div>}<p className="text-sm text-muted">Esta accion elimina del dispositivo el movimiento pendiente de este usuario.</p>
      </Modal>
    </main>
  );
}
