import { useEffect, useRef, useState } from "react";
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
import { Button, EmptyState, Field, Modal, Spinner } from "../../components/ui";
import { errorMessage, isAppError, isSessionRequired } from "../../lib/app-error";
import { formatDateTime, formatMoney } from "../../lib/format";
import { useUpdater, type CartReportVersion } from "../updater/UpdaterContext";
import type { Category } from "../../types/category";
import type { Cart, CashChangeQuote } from "../../types/cart";
import type { ProductSelectorRow } from "../../types/product";
import type { PaymentMethod, SaleReceipt } from "../../types/sale";
import type { User } from "../../types/user";
import {
  checkoutInput,
  clearPendingCheckout,
  readPendingCheckout,
  savePendingCheckout,
  type PendingCheckout,
} from "./checkout-intent";

type Notice = { tone: "success" | "error" | "info"; message: string };
type PosViewProps = {
  user: User;
  sessionId: number;
  onNotice: (notice: Notice) => void;
  onSessionRequired: () => void;
};

const paymentMethods: { value: PaymentMethod; label: string }[] = [
  { value: "efectivo", label: "Efectivo" },
  { value: "tarjeta", label: "Tarjeta" },
  { value: "transferencia", label: "Transferencia" },
];

const clearedCart: Cart = {
  lines: [],
  item_count: 0,
  subtotal: 0,
  total: 0,
  cart_fingerprint: "",
};

function uncertainCheckoutError(error: unknown) {
  return !isAppError(error) || error.category === "database" || error.category === "internal" || isSessionRequired(error);
}

function checkoutIntentConflict(error: unknown) {
  return isAppError(error) && ["CART_CHANGED", "CART_FINGERPRINT_MISMATCH", "CHECKOUT_TOKEN_REUSED"].includes(error.code);
}

export function PosView({ user, sessionId, onNotice, onSessionRequired }: PosViewProps) {
  const { beginCartReporting, beginCartOperation, reportCart, runCartMutation, runCheckout } = useUpdater();
  const [cart, setCart] = useState<Cart>();
  const [categories, setCategories] = useState<Category[]>([]);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerItems, setPickerItems] = useState<ProductSelectorRow[]>([]);
  const [pickerSearch, setPickerSearch] = useState("");
  const [pickerCategory, setPickerCategory] = useState("");
  const [pickerLoading, setPickerLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");
  const [dataReady, setDataReady] = useState(false);
  const [dataStale, setDataStale] = useState(false);
  const [checkoutError, setCheckoutError] = useState("");
  const [paymentMethod, setPaymentMethod] = useState<PaymentMethod>("efectivo");
  const [cashReceived, setCashReceived] = useState("");
  const [cashQuote, setCashQuote] = useState<CashChangeQuote>();
  const [cashQuoteError, setCashQuoteError] = useState("");
  const [quoteLoading, setQuoteLoading] = useState(false);
  const [receipt, setReceipt] = useState<SaleReceipt>();
  const [pending, setPending] = useState<PendingCheckout>();
  const [intentConflict, setIntentConflict] = useState(false);
  const [discardOpen, setDiscardOpen] = useState(false);
  const recoveryStarted = useRef("");
  const mutationInFlight = useRef<symbol | undefined>(undefined);
  const pendingRef = useRef<PendingCheckout | undefined>(undefined);
  const pickerRequest = useRef(0);
  const loadRequest = useRef(0);
  const quoteRequest = useRef(0);
  const lifetime = useRef(0);
  const cartReporting = useRef<{ sessionId: number; generation: number; operation: number } | undefined>(undefined);
  if (!cartReporting.current || cartReporting.current.sessionId !== sessionId) {
    cartReporting.current = { sessionId, generation: beginCartReporting(sessionId), operation: 0 };
  }
  pendingRef.current = pending;
  const busyOrPending = busy || Boolean(pending);

  useEffect(() => {
    const generation = ++lifetime.current;
    return () => {
      if (lifetime.current === generation) lifetime.current += 1;
      mutationInFlight.current = undefined;
      loadRequest.current += 1;
      pickerRequest.current += 1;
      quoteRequest.current += 1;
    };
  }, [sessionId]);

  function nextCartOperation(): CartReportVersion {
    const reporting = cartReporting.current!;
    const version = { generation: reporting.generation, operation: ++reporting.operation };
    beginCartOperation(sessionId, version);
    return version;
  }

  function reportResolvedCart(version: CartReportVersion, nextCart: Cart) {
    reportCart(sessionId, version, nextCart.lines.length > 0);
  }

  function handleError(value: unknown, fallback: string) {
    if (isSessionRequired(value)) {
      onSessionRequired();
      return;
    }
    onNotice({ tone: "error", message: errorMessage(value, fallback) });
  }

  function closePicker() {
    pickerRequest.current += 1;
    setPickerLoading(false);
    setPickerOpen(false);
  }

  async function refreshPicker() {
    if (!pickerOpen) return;
    const generation = lifetime.current;
    const request = ++pickerRequest.current;
    setPickerLoading(true);
    try {
      const items = await listProductSelector({
        search: pickerSearch || undefined,
        category_id: pickerCategory ? Number(pickerCategory) : undefined,
      });
      if (lifetime.current === generation && pickerRequest.current === request) setPickerItems(items);
    } catch (value: unknown) {
      if (lifetime.current === generation && pickerRequest.current === request) handleError(value, "No se pudo consultar el catalogo.");
    } finally {
      if (lifetime.current === generation && pickerRequest.current === request) setPickerLoading(false);
    }
  }

  async function recover(intent: PendingCheckout) {
    if (mutationInFlight.current || intent.userId !== user.id) return;
    const generation = lifetime.current;
    const mutation = Symbol();
    mutationInFlight.current = mutation;
    setBusy(true);
    setPending(intent);
    setIntentConflict(false);
    setCheckoutError("");
    closePicker();
    const checkoutCartOperation = nextCartOperation();
    try {
      const recovered = await runCheckout(sessionId, () => checkoutCart(checkoutInput(intent)));
      reportCart(sessionId, checkoutCartOperation, false);
      if (lifetime.current !== generation) return;
      clearPendingCheckout(intent.userId, intent.checkoutToken);
      setPending(undefined);
      setIntentConflict(false);
      setReceipt(recovered);
      setCart(clearedCart);
      setPickerItems([]);
      try {
        const refreshOperation = nextCartOperation();
        const nextCart = await getCart();
        reportResolvedCart(refreshOperation, nextCart);
        if (lifetime.current !== generation) return;
        setCart(nextCart);
        setLoadError("");
        setDataStale(false);
        onNotice({ tone: "success", message: "Cobro recuperado correctamente." });
      } catch {
        if (lifetime.current !== generation) return;
        setLoadError("No se pudo actualizar el carrito despues de la venta.");
        setDataStale(true);
        onNotice({ tone: "error", message: "Venta completada, no se pudo actualizar el carrito." });
      }
    } catch (value: unknown) {
      if (lifetime.current !== generation) return;
      if (checkoutIntentConflict(value)) {
        setIntentConflict(true);
      } else if (!uncertainCheckoutError(value)) {
        clearPendingCheckout(intent.userId, intent.checkoutToken);
        setPending(undefined);
      }
      if (isSessionRequired(value)) onSessionRequired();
      else if (checkoutIntentConflict(value)) setCheckoutError("El carrito o la intencion ya no coinciden. No se realizara un cobro con datos distintos.");
      else setCheckoutError(errorMessage(value, "No fue posible confirmar el estado del cobro. Usa Reintentar cobro."));
    } finally {
      if (lifetime.current === generation && mutationInFlight.current === mutation) {
        mutationInFlight.current = undefined;
        setBusy(false);
      }
    }
  }

  async function loadData() {
    const generation = lifetime.current;
    const request = ++loadRequest.current;
    const preservingData = dataReady;
    setLoading(true);
    setLoadError("");
    const cartOperation = nextCartOperation();
    try {
      const [nextCart, nextCategories] = await Promise.all([getCart(), listCategories()]);
      reportResolvedCart(cartOperation, nextCart);
      if (lifetime.current !== generation || loadRequest.current !== request) return;
      setCart(nextCart);
      setCategories(nextCategories);
      setDataReady(true);
      setDataStale(false);
      const stored = readPendingCheckout(user.id);
      const recoveryKey = stored ? `${stored.userId}:${stored.checkoutToken}` : "";
      if (stored && recoveryStarted.current !== recoveryKey) {
        recoveryStarted.current = recoveryKey;
        setPending(stored);
        if (nextCart.lines.length > 0 && nextCart.cart_fingerprint !== stored.cartFingerprint) {
          setIntentConflict(true);
          setCheckoutError("El carrito cambio desde que se inicio el cobro. Revisa los datos y descarta la intencion para continuar.");
        } else {
          void recover(stored);
        }
      }
    } catch (value: unknown) {
      if (lifetime.current !== generation || loadRequest.current !== request) return;
      if (isSessionRequired(value)) onSessionRequired();
      else {
        setLoadError(errorMessage(value, "No se pudo cargar el punto de venta."));
        if (preservingData) setDataStale(true);
      }
    } finally {
      if (lifetime.current === generation && loadRequest.current === request) setLoading(false);
    }
  }

  useEffect(() => {
    void loadData();
  }, [sessionId]);

  useEffect(() => {
    const request = ++quoteRequest.current;
    setCashQuote(undefined);
    setCashQuoteError("");
    setQuoteLoading(false);
    const validCart = dataReady && !dataStale && cart && cart.lines.length > 0 && cart.lines.every((line) => line.available);
    if (paymentMethod !== "efectivo" || !validCart || !cashReceived.trim() || pending) return;

    setQuoteLoading(true);
    const timeout = window.setTimeout(() => {
      void quoteCashChange(cashReceived)
        .then((quote) => {
          if (quoteRequest.current !== request) return;
          if (quote.total !== cart.total) {
            setCashQuoteError("El total del carrito cambio. Recarga los datos antes de cobrar.");
            return;
          }
          setCashQuote(quote);
        })
        .catch((value: unknown) => {
          if (quoteRequest.current !== request) return;
          if (isSessionRequired(value)) onSessionRequired();
          else if (isAppError(value) && value.code === "INSUFFICIENT_CASH") setCashQuoteError("Efectivo insuficiente.");
          else setCashQuoteError(errorMessage(value, "No se pudo calcular el cambio de forma segura."));
        })
        .finally(() => {
          if (quoteRequest.current === request) setQuoteLoading(false);
        });
    }, 250);
    return () => window.clearTimeout(timeout);
  }, [cashReceived, paymentMethod, cart?.cart_fingerprint, dataReady, dataStale, pending]);

  useEffect(() => {
    if (!pickerOpen) return;
    const timeout = window.setTimeout(() => void refreshPicker(), 250);
    return () => {
      window.clearTimeout(timeout);
      pickerRequest.current += 1;
      setPickerLoading(false);
    };
  }, [pickerOpen, pickerSearch, pickerCategory, cart?.cart_fingerprint]);

  useEffect(() => {
    function openPicker(event: KeyboardEvent) {
      const target = event.target;
      const writing = target instanceof HTMLElement && (
        target.matches("input, textarea, select") || target.isContentEditable
      );
      if (event.key === "F2" && !writing && !document.body.classList.contains("modal-scroll-lock") && !busyOrPending) {
        event.preventDefault();
        setPickerOpen(true);
      }
    }
    window.addEventListener("keydown", openPicker);
    return () => window.removeEventListener("keydown", openPicker);
  }, [busyOrPending]);

  async function mutate(operation: () => Promise<Cart>, fallback: string) {
    if (mutationInFlight.current || pendingRef.current) return;
    const generation = lifetime.current;
    const mutation = Symbol();
    const cartOperation = nextCartOperation();
    mutationInFlight.current = mutation;
    setBusy(true);
    try {
      const nextCart = await runCartMutation(sessionId, cartOperation, operation);
      reportResolvedCart(cartOperation, nextCart);
      if (lifetime.current !== generation) return;
      setCart(nextCart);
      setDataStale(false);
    } catch (value: unknown) {
      if (lifetime.current === generation) handleError(value, fallback);
    } finally {
      if (lifetime.current === generation && mutationInFlight.current === mutation) {
        mutationInFlight.current = undefined;
        setBusy(false);
      }
    }
  }

  async function addProduct(productId: number) {
    await mutate(() => addCartItem(productId), "No se pudo agregar el producto.");
  }

  async function submitCheckout() {
    if (!cart || mutationInFlight.current || pendingRef.current) return;
    const generation = lifetime.current;
    const mutation = Symbol();
    mutationInFlight.current = mutation;
    const intent: PendingCheckout = {
      version: 1,
      userId: user.id,
      checkoutToken: crypto.randomUUID(),
      cartFingerprint: cart.cart_fingerprint,
      paymentMethod,
      ...(paymentMethod === "efectivo" ? { cashReceived } : {}),
    };
    try {
      savePendingCheckout(intent);
    } catch {
      if (mutationInFlight.current === mutation) mutationInFlight.current = undefined;
      onNotice({ tone: "error", message: "No se pudo proteger el cobro localmente. Intenta de nuevo." });
      return;
    }
    setCheckoutError("");
    setPending(intent);
    setBusy(true);
    closePicker();
    const checkoutCartOperation = nextCartOperation();
    try {
      const nextReceipt = await runCheckout(sessionId, () => checkoutCart(checkoutInput(intent)));
      reportCart(sessionId, checkoutCartOperation, false);
      if (lifetime.current !== generation) return;
      clearPendingCheckout(intent.userId, intent.checkoutToken);
      setPending(undefined);
      setIntentConflict(false);
      setReceipt(nextReceipt);
      setCashReceived("");
      setCart(clearedCart);
      setPickerItems([]);
      try {
        const refreshOperation = nextCartOperation();
        const nextCart = await getCart();
        reportResolvedCart(refreshOperation, nextCart);
        if (lifetime.current !== generation) return;
        setCart(nextCart);
        setLoadError("");
        setDataStale(false);
        onNotice({ tone: "success", message: "Venta completada correctamente." });
      } catch {
        if (lifetime.current !== generation) return;
        setLoadError("No se pudo actualizar el carrito despues de la venta.");
        setDataStale(true);
        onNotice({ tone: "error", message: "Venta completada, no se pudo actualizar el carrito." });
      }
    } catch (value: unknown) {
      if (lifetime.current !== generation) return;
      if (checkoutIntentConflict(value)) {
        setIntentConflict(true);
      } else if (!uncertainCheckoutError(value)) {
        clearPendingCheckout(intent.userId, intent.checkoutToken);
        setPending(undefined);
      }
      if (isSessionRequired(value)) onSessionRequired();
      else if (checkoutIntentConflict(value)) setCheckoutError("El carrito cambio durante el cobro. La intencion se conservo y no se cobro un precio distinto.");
      else setCheckoutError(errorMessage(value, "No fue posible confirmar el cobro. La intencion se conservo para reintentar."));
    } finally {
      if (lifetime.current === generation && mutationInFlight.current === mutation) {
        mutationInFlight.current = undefined;
        setBusy(false);
      }
    }
  }

  async function discardIntent() {
    if (!pending) return;
    clearPendingCheckout(pending.userId, pending.checkoutToken);
    setPending(undefined);
    setIntentConflict(false);
    setDiscardOpen(false);
    setCheckoutError("");
    recoveryStarted.current = "";
    await loadData();
  }

  if (loading && !dataReady) return <main id="main-content" tabIndex={-1} className="grid min-h-[calc(100dvh-88px)] place-items-center"><Spinner className="size-6 text-brand" /></main>;

  if (!dataReady) return <main id="main-content" tabIndex={-1} className="grid min-h-[calc(100dvh-88px)] place-items-center p-6"><section className="w-full max-w-md rounded-card border border-line bg-surface p-6 text-center" role="alert"><h2 className="font-heading text-xl font-bold">No se pudo cargar el punto de venta</h2><p className="mt-2 text-sm text-danger">{loadError}</p><Button className="mt-5" onClick={() => void loadData()}>Reintentar</Button></section></main>;

  return (
    <main id="main-content" tabIndex={-1} className="p-4 outline-none sm:p-6 min-[900px]:p-8">
      {pending && (
        <section className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-card border border-warning/30 bg-warning-soft p-4" role="status">
          <div><strong className="text-sm text-ink">Cobro pendiente de confirmar</strong><p className="mt-1 text-xs text-muted">La intencion permanece guardada y puede recuperar el recibo sin volver a cobrar.</p></div>
          <div className="flex flex-wrap gap-2"><Button disabled={busy || intentConflict} onClick={() => void recover(pending)}>Reintentar cobro</Button>{intentConflict && <Button variant="ghost" disabled={busy} onClick={() => setDiscardOpen(true)}>Descartar intencion</Button>}</div>
        </section>
      )}
      {loadError && dataStale && <div className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-control border border-warning/30 bg-warning-soft px-4 py-3 text-sm text-warning" role="status"><span>No se pudieron actualizar los datos. Se muestran datos anteriores.</span><Button size="sm" disabled={loading} onClick={() => void loadData()}>Reintentar</Button></div>}
      {checkoutError && <div className="mb-4 rounded-control border border-danger/25 bg-danger-soft px-4 py-3 text-sm text-danger" role="alert">{checkoutError}</div>}
      <div className="grid min-h-[calc(100dvh-10rem)] gap-5 xl:grid-cols-[minmax(0,1fr)_400px]">
        <section className="min-w-0 rounded-card border border-line bg-surface p-4 sm:p-6">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div><h2 id="cart-table-title" className="font-heading text-xl font-bold">Carrito actual</h2><p className="mt-1 text-xs text-muted">{cart?.item_count ?? 0} articulos en esta venta</p></div>
            <div className="flex gap-2"><Button variant="ghost" disabled={busyOrPending || !cart?.lines.length} onClick={() => void mutate(clearCart, "No se pudo vaciar el carrito.")}>Vaciar</Button><Button variant="primary" disabled={busyOrPending} onClick={() => setPickerOpen(true)}>Agregar producto <kbd className="rounded bg-white/15 px-1.5 py-0.5 text-[0.625rem]">F2</kbd></Button></div>
          </div>
          {!cart?.lines.length ? (
            <EmptyState title="El carrito esta vacio" description="Abre el selector para agregar productos disponibles." action={<Button disabled={busyOrPending} onClick={() => setPickerOpen(true)}>Buscar productos</Button>} />
          ) : (
            <div className="mt-5 overflow-x-auto">
              <table aria-labelledby="cart-table-title" className="w-full min-w-[620px] border-separate border-spacing-0 text-left text-sm">
                <caption className="sr-only">Productos agregados al carrito actual</caption>
                <thead><tr className="text-[0.625rem] tracking-wider text-muted uppercase"><th className="border-b border-line px-3 py-3">Producto</th><th className="border-b border-line px-3 py-3">Precio</th><th className="border-b border-line px-3 py-3 text-center">Cantidad</th><th className="border-b border-line px-3 py-3 text-right">Subtotal</th><th className="border-b border-line px-3 py-3"><span className="sr-only">Acciones</span></th></tr></thead>
                <tbody>{cart.lines.map((line) => <tr key={line.product_id} className={!line.available ? "bg-danger-soft/50" : ""}><td className="border-b border-line px-3 py-4"><strong className="block">{line.name}</strong><span className="text-xs text-muted">{line.sku} · stock {line.stock}</span></td><td className="border-b border-line px-3 py-4">{formatMoney(line.unit_price)}</td><td className="border-b border-line px-3 py-4"><div className="mx-auto flex w-fit items-center rounded-control border border-line"><button aria-label={`Reducir ${line.name}`} disabled={busyOrPending} className="size-9 text-lg disabled:opacity-40" onClick={() => void mutate(() => decrementCartItem(line.product_id), "No se pudo reducir la cantidad.")}>−</button><span className="min-w-8 text-center font-bold">{line.quantity}</span><button aria-label={`Aumentar ${line.name}`} disabled={busyOrPending} className="size-9 text-lg disabled:opacity-40" onClick={() => void mutate(() => incrementCartItem(line.product_id), "No se pudo aumentar la cantidad.")}>+</button></div></td><td className="border-b border-line px-3 py-4 text-right font-bold">{formatMoney(line.subtotal)}</td><td className="border-b border-line px-3 py-4"><button aria-label={`Quitar ${line.name}`} disabled={busyOrPending} className="rounded p-2 text-danger hover:bg-danger-soft disabled:opacity-40" onClick={() => void mutate(() => removeCartItem(line.product_id), "No se pudo quitar el producto.")}>Quitar</button></td></tr>)}</tbody>
              </table>
            </div>
          )}
        </section>

        <aside className="h-fit rounded-card border border-line bg-surface p-5 shadow-sm sm:p-6">
          <h2 className="font-heading text-xl font-bold">Cobro</h2>
          <dl className="mt-5 grid gap-3 border-b border-line pb-5 text-sm"><div className="flex justify-between"><dt className="text-muted">Articulos</dt><dd className="font-semibold">{cart?.item_count ?? 0}</dd></div><div className="flex justify-between"><dt className="text-muted">Subtotal</dt><dd className="font-semibold">{formatMoney(cart?.subtotal ?? 0)}</dd></div><div className="flex items-end justify-between pt-2"><dt className="font-bold">Total</dt><dd className="font-heading text-3xl font-bold text-brand">{formatMoney(cart?.total ?? 0)}</dd></div></dl>
           <fieldset className="mt-5" disabled={busyOrPending}><legend className="mb-2 text-[0.625rem] font-extrabold tracking-wider text-muted uppercase">Metodo de pago</legend><div className="grid grid-cols-3 gap-2">{paymentMethods.map((method) => <label key={method.value} className={`cursor-pointer rounded-control border px-2 py-3 text-center text-xs font-bold transition has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-brand ${paymentMethod === method.value ? "border-brand bg-brand-soft text-brand" : "border-line text-muted"}`}><input className="sr-only" type="radio" name="payment" value={method.value} checked={paymentMethod === method.value} onChange={() => setPaymentMethod(method.value)} />{method.label}</label>)}</div></fieldset>
           {paymentMethod === "efectivo" && <div className="mt-5"><Field label="Efectivo recibido" value={cashReceived} onChange={(event) => setCashReceived(event.target.value)} inputMode="decimal" placeholder="0.00" disabled={busyOrPending} required /></div>}
           {paymentMethod === "efectivo" && cashReceived.trim() && <div className="mt-3 min-h-16" aria-live="polite">{quoteLoading ? <p className="text-xs font-semibold text-muted">Calculando cambio...</p> : cashQuote ? <dl className="grid gap-2 rounded-control bg-surface-soft p-3 text-sm"><div className="flex justify-between"><dt className="text-muted">Total</dt><dd className="font-semibold">{formatMoney(cashQuote.total)}</dd></div><div className="flex justify-between"><dt className="text-muted">Efectivo</dt><dd className="font-semibold">{formatMoney(cashQuote.cash_received)}</dd></div><div className="flex justify-between border-t border-line pt-2"><dt className="font-bold">Cambio</dt><dd className="font-bold text-brand">{formatMoney(cashQuote.change)}</dd></div></dl> : cashQuoteError ? <p className="rounded-control bg-danger-soft px-3 py-2 text-sm text-danger" role="status">{cashQuoteError}</p> : null}</div>}
           <Button className="mt-5 w-full" size="lg" variant="primary" loading={busy} disabled={busyOrPending || !cart?.lines.length || dataStale || (paymentMethod === "efectivo" && !cashQuote)} onClick={() => void submitCheckout()}>Cobrar {formatMoney(cart?.total ?? 0)}</Button>
        </aside>
      </div>

      <Modal open={pickerOpen} onClose={closePicker} closeDisabled={busy} title="Agregar productos" description="La disponibilidad considera las unidades que ya estan en el carrito." size="lg">
        <div className="grid gap-3 sm:grid-cols-[1fr_220px]"><Field data-autofocus label="Buscar" value={pickerSearch} onChange={(event) => setPickerSearch(event.target.value)} placeholder="Nombre o SKU" disabled={busyOrPending} /><label className="grid gap-2 text-[0.625rem] font-extrabold tracking-wider text-muted uppercase">Categoria<select value={pickerCategory} onChange={(event) => setPickerCategory(event.target.value)} disabled={busyOrPending} className="h-11 rounded-control border border-line-strong bg-surface px-3 text-sm font-normal tracking-normal text-ink normal-case outline-none focus:border-brand disabled:opacity-45"><option value="">Todas</option>{categories.map((category) => <option key={category.id} value={category.id}>{category.name}</option>)}</select></label></div>
        <div className="mt-5 min-h-56">{pickerLoading ? <Spinner className="mx-auto mt-20 size-6 text-brand" /> : pickerItems.length ? <div className="grid gap-2 sm:grid-cols-2">{pickerItems.map((product) => <article key={product.id} className="flex items-center justify-between gap-3 rounded-card border border-line p-4"><div className="min-w-0"><strong className="block truncate text-sm">{product.name}</strong><span className="block truncate text-xs text-muted">{product.sku} · {product.category_name}</span><span className="mt-2 block text-sm font-bold text-brand">{formatMoney(product.price)}</span></div><Button size="sm" disabled={busyOrPending || product.available_stock === 0} onClick={() => void addProduct(product.id)}>{product.available_stock === 0 ? "Sin stock" : `Agregar · ${product.available_stock}`}</Button></article>)}</div> : <EmptyState className="min-h-56" title="No hay productos disponibles" description="Prueba con otra busqueda o categoria." />}</div>
      </Modal>

      <Modal open={Boolean(receipt)} onClose={() => setReceipt(undefined)} title="Venta completada" description={receipt ? formatDateTime(receipt.sale.sale_date) : undefined} size="sm" footer={<Button variant="primary" onClick={() => setReceipt(undefined)}>Cerrar recibo</Button>}>
        {receipt && <div><div className="rounded-card bg-brand-soft p-5 text-center"><span className="text-xs font-bold tracking-wider text-brand uppercase">Ticket POS-{String(receipt.sale.id).padStart(8, "0")}</span><strong className="mt-2 block font-heading text-3xl text-brand-dark">{formatMoney(receipt.sale.total)}</strong><span className="mt-1 block text-xs text-muted capitalize">{receipt.sale.payment_method}</span></div><div className="mt-4 divide-y divide-line">{receipt.details.map((detail) => <div key={detail.id} className="flex justify-between gap-4 py-3 text-sm"><span><strong className="block">{detail.product_name}</strong><span className="text-xs text-muted">{detail.sku} · {detail.quantity} unidades</span></span><span className="shrink-0 text-muted">{formatMoney(detail.unit_price)} c/u</span></div>)}</div>{receipt.sale.change_amount !== null && <div className="mt-4 flex justify-between border-t border-line pt-4 text-sm font-bold"><span>Cambio</span><span>{formatMoney(receipt.sale.change_amount)}</span></div>}</div>}
      </Modal>

      <Modal open={discardOpen} onClose={() => setDiscardOpen(false)} closeDisabled={busy} title="Descartar intencion de cobro" description="Esta accion no confirma ni repite el cobro." size="sm" footer={<><Button disabled={busy} onClick={() => setDiscardOpen(false)}>Cancelar</Button><Button variant="primary" loading={busy} onClick={() => void discardIntent()}>Descartar y recargar carrito</Button></>}>
        <p className="text-sm leading-6 text-muted">Descarta la intencion guardada solo si verificaste que la venta no fue cobrada. El carrito se recargara con los precios actuales.</p>
      </Modal>
    </main>
  );
}
