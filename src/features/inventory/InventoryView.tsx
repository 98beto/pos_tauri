import { useEffect, useRef, useState, type FormEvent } from "react";
import { createInventoryMovement, getInventoryView } from "../../api/inventory";
import { listProducts } from "../../api/products";
import { Button, EmptyState, Field, Modal, Spinner, StatCard } from "../../components/ui";
import { errorMessage, isAppError, isSessionRequired } from "../../lib/app-error";
import { formatDateTime } from "../../lib/format";
import type {
  CreateInventoryMovementInput,
  InventoryMovement,
  InventoryView as InventoryViewData,
} from "../../types/inventory_movement";
import type { Product } from "../../types/product";
import type { User } from "../../types/user";
import {
  clearPendingManualMovement,
  manualMovementInput,
  readPendingManualMovement,
  savePendingManualMovement,
  type PendingManualMovement,
} from "./manual-movement-intent";

type Notice = { tone: "success" | "error" | "info"; message: string };
type InventoryViewProps = {
  user: User;
  onNotice: (notice: Notice) => void;
  onSessionRequired: () => void;
};

type Operation = "purchase" | "adjustment-in" | "adjustment-out";
type LoadResult = "success" | "error" | "session" | "stale";

const operations: Record<Operation, Pick<CreateInventoryMovementInput, "reason" | "type">> = {
  purchase: { reason: "compra", type: "entrada" },
  "adjustment-in": { reason: "ajuste", type: "entrada" },
  "adjustment-out": { reason: "ajuste", type: "salida" },
};

const movementRequests = new Map<string, Promise<InventoryMovement>>();

function requestKey(intent: PendingManualMovement) {
  return `${intent.userId}:${intent.operationToken}`;
}

function submitMovementOnce(intent: PendingManualMovement) {
  const key = requestKey(intent);
  const active = movementRequests.get(key);
  if (active) return active;
  const request = createInventoryMovement(manualMovementInput(intent));
  movementRequests.set(key, request);
  void request.then(
    () => movementRequests.delete(key),
    () => movementRequests.delete(key),
  );
  return request;
}

function operationFor(intent: PendingManualMovement): Operation {
  if (intent.reason === "compra") return "purchase";
  return intent.type === "entrada" ? "adjustment-in" : "adjustment-out";
}

const selectClass = "h-11 w-full rounded-control border border-line-strong bg-surface px-3 text-sm text-ink outline-none transition focus:border-brand focus:ring-3 focus:ring-brand/12 disabled:bg-surface-soft";

const reasonLabels: Record<InventoryMovement["reason"], string> = {
  compra: "Compra",
  venta: "Venta",
  ajuste: "Ajuste",
};

const reasonClasses: Record<InventoryMovement["reason"], string> = {
  compra: "bg-info-soft text-info",
  venta: "bg-warning-soft text-warning",
  ajuste: "bg-brand-soft text-brand",
};

export function InventoryView({ user, onNotice, onSessionRequired }: InventoryViewProps) {
  const [view, setView] = useState<InventoryViewData>();
  const [search, setSearch] = useState("");
  const [reason, setReason] = useState("");
  const [movementType, setMovementType] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [movementOpen, setMovementOpen] = useState(false);
  const [products, setProducts] = useState<Product[]>([]);
  const [productsLoading, setProductsLoading] = useState(false);
  const [productsError, setProductsError] = useState("");
  const [productId, setProductId] = useState("");
  const [operation, setOperation] = useState<Operation>("purchase");
  const [quantity, setQuantity] = useState("");
  const [movementError, setMovementError] = useState("");
  const [quantityError, setQuantityError] = useState("");
  const [saving, setSaving] = useState(false);
  const [pendingIntent, setPendingIntent] = useState<PendingManualMovement | null>(() => readPendingManualMovement(user.id));
  const [intentCanBeDiscarded, setIntentCanBeDiscarded] = useState(false);
  const [cleanupRequired, setCleanupRequired] = useState(false);
  const [discardOpen, setDiscardOpen] = useState(false);
  const [discardError, setDiscardError] = useState("");
  const viewRequest = useRef(0);
  const productsRequest = useRef(0);
  const saveInFlight = useRef(false);
  const quantityRef = useRef<HTMLInputElement>(null);
  const mounted = useRef(true);
  const lifetime = useRef(0);

  async function loadView(): Promise<LoadResult> {
    const request = ++viewRequest.current;
    setLoading(true);
    setError("");
    try {
      const nextView = await getInventoryView({
        search: search || undefined,
        reason: (reason || undefined) as InventoryMovement["reason"] | undefined,
        type: (movementType || undefined) as InventoryMovement["type"] | undefined,
      });
      if (viewRequest.current !== request) return "stale";
      setView(nextView);
      return "success";
    } catch (value: unknown) {
      if (viewRequest.current !== request) return "stale";
      if (isSessionRequired(value)) {
        onSessionRequired();
        return "session";
      }
      setError(errorMessage(value, "No se pudo consultar el inventario."));
      return "error";
    } finally {
      if (viewRequest.current === request) setLoading(false);
    }
  }

  useEffect(() => {
    setLoading(true);
    setError("");
    const timeout = window.setTimeout(() => void loadView(), 250);
    return () => {
      window.clearTimeout(timeout);
      viewRequest.current += 1;
    };
  }, [search, reason, movementType]);

  useEffect(() => {
    mounted.current = true;
    lifetime.current += 1;
    return () => {
      mounted.current = false;
      lifetime.current += 1;
      productsRequest.current += 1;
    };
  }, []);

  useEffect(() => {
    setPendingIntent(readPendingManualMovement(user.id));
    setIntentCanBeDiscarded(false);
    setCleanupRequired(false);
    setDiscardOpen(false);
    setDiscardError("");
  }, [user.id]);

  async function loadMovementProducts() {
    const request = ++productsRequest.current;
    setProductsLoading(true);
    setProductsError("");
    try {
      const nextProducts = await listProducts();
      if (productsRequest.current !== request) return;
      setProducts(nextProducts);
      setProductId((current) => current || nextProducts[0]?.id.toString() || "");
    } catch (value: unknown) {
      if (productsRequest.current !== request) return;
      if (isSessionRequired(value)) {
        closeMovement();
        onSessionRequired();
      } else {
        setProductsError(errorMessage(value, "No se pudieron cargar los productos."));
      }
    } finally {
      if (productsRequest.current === request) setProductsLoading(false);
    }
  }

  function openMovement() {
    setProductId("");
    setOperation("purchase");
    setQuantity("");
    setMovementError("");
    setQuantityError("");
    setProductsError("");
    setProducts([]);
    setMovementOpen(true);
    void loadMovementProducts();
  }

  function closeMovement() {
    if (saveInFlight.current) return;
    productsRequest.current += 1;
    setMovementOpen(false);
    setProductsLoading(false);
    setProductId("");
    setQuantity("");
    setMovementError("");
    setQuantityError("");
    setProductsError("");
  }

  async function submitIntent(intent: PendingManualMovement) {
    if (saveInFlight.current || intent.userId !== user.id) return;
    const requestLifetime = lifetime.current;
    saveInFlight.current = true;
    setSaving(true);
    setMovementError("");
    setQuantityError("");
    setIntentCanBeDiscarded(false);
    let saved = false;
    try {
      await submitMovementOnce(intent);
      if (!mounted.current || lifetime.current !== requestLifetime) return;
      const cleanup = clearPendingManualMovement(intent.userId, intent.operationToken);
      if (cleanup !== "cleared" && cleanup !== "absent") {
        const message = "El movimiento fue confirmado, pero no se pudo limpiar la recuperacion local. Reintenta la limpieza de forma segura.";
        setCleanupRequired(true);
        setMovementError(message);
        onNotice({ tone: "info", message });
        return;
      }
      setPendingIntent(null);
      saved = true;
    } catch (value: unknown) {
      if (!mounted.current || lifetime.current !== requestLifetime) return;
      if (isSessionRequired(value)) {
        setMovementOpen(false);
        onSessionRequired();
      } else {
        const message = errorMessage(value, "No se pudo confirmar el movimiento. La intencion se conservo para reintentar.");
        if (isAppError(value) && value.field === "quantity") {
          setQuantityError(message);
          window.requestAnimationFrame(() => quantityRef.current?.focus());
        } else {
          setMovementError(message);
        }
        setIntentCanBeDiscarded(isAppError(value) && !["database", "internal", "authentication"].includes(value.category));
      }
    } finally {
      saveInFlight.current = false;
      if (mounted.current) setSaving(false);
    }
    if (!saved || !mounted.current) return;

    setMovementOpen(false);
    setProductId("");
    setQuantity("");
    onNotice({ tone: "success", message: "Movimiento de inventario registrado." });
    if (await loadView() === "error") {
      onNotice({ tone: "info", message: "El movimiento se guardo, pero no se pudo recargar el inventario; usa Reintentar." });
    }
  }

  function recoverIntent(intent: PendingManualMovement) {
    setProductId(intent.productId.toString());
    setOperation(operationFor(intent));
    setQuantity(intent.quantity);
    setMovementError("");
    setQuantityError("");
    setProductsError("");
    setMovementOpen(true);
    if (!products.length) void loadMovementProducts();
    void submitIntent(intent);
  }

  async function retryCleanup() {
    if (!pendingIntent) return;
    const cleanup = clearPendingManualMovement(pendingIntent.userId, pendingIntent.operationToken);
    if (cleanup !== "cleared" && cleanup !== "absent") {
      setMovementError("El movimiento sigue confirmado, pero no se pudo limpiar la recuperacion local.");
      return;
    }
    setPendingIntent(null);
    setCleanupRequired(false);
    setMovementOpen(false);
    setMovementError("");
    onNotice({ tone: "success", message: "Recuperacion local limpiada; el movimiento ya estaba confirmado." });
    await loadView();
  }

  async function saveMovement(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saveInFlight.current) return;
    if (cleanupRequired) {
      await retryCleanup();
      return;
    }
    if (pendingIntent) {
      await submitIntent(pendingIntent);
      return;
    }
    if (!productId) {
      setMovementError("Selecciona un producto.");
      return;
    }
    const intent: PendingManualMovement = {
      version: 1,
      userId: user.id,
      operationToken: crypto.randomUUID(),
      productId: Number(productId),
      ...operations[operation],
      quantity,
    };
    try {
      savePendingManualMovement(intent);
    } catch {
      setMovementError("No se pudo proteger la operacion localmente. Intenta de nuevo.");
      return;
    }
    setPendingIntent(intent);
    await submitIntent(intent);
  }

  async function discardIntent() {
    if (!pendingIntent) return;
    const cleanup = clearPendingManualMovement(pendingIntent.userId, pendingIntent.operationToken);
    if (cleanup !== "cleared" && cleanup !== "absent") {
      setDiscardError("No se pudo eliminar la recuperacion local. La intencion se conservo.");
      return;
    }
    setPendingIntent(null);
    setIntentCanBeDiscarded(false);
    setDiscardOpen(false);
    setDiscardError("");
    setMovementOpen(false);
    setMovementError("");
    setQuantityError("");
    await loadView();
  }

  function confirmDiscardIntent() {
    setMovementOpen(false);
    setDiscardError("");
    setDiscardOpen(true);
  }

  const hasFilters = Boolean(search || reason || movementType);
  const statValue = (value: number | undefined) => value === undefined ? "-" : value;

  function updateFilter(setter: (value: string) => void, value: string) {
    setLoading(true);
    setError("");
    setter(value);
  }

  return (
    <main id="main-content" tabIndex={-1} className="p-4 outline-none sm:p-6 min-[900px]:p-8">
      {pendingIntent && (
        <section className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-card border border-warning/30 bg-warning-soft p-4" role="status">
          <div><strong className="text-sm text-ink">{cleanupRequired ? "Movimiento confirmado con recuperacion local pendiente" : "Movimiento pendiente de confirmar"}</strong><p className="mt-1 text-xs text-muted">{cleanupRequired ? "No se repetira el movimiento; solo falta limpiar la recuperacion guardada." : "La intencion esta guardada para este usuario y se reintentara con el mismo token."}</p></div>
          <div className="flex flex-wrap gap-2">{cleanupRequired ? <Button disabled={saving} onClick={() => void retryCleanup()}>Reintentar limpieza</Button> : <Button disabled={saving} onClick={() => recoverIntent(pendingIntent)}>Reintentar movimiento</Button>}{intentCanBeDiscarded && !cleanupRequired && <Button variant="ghost" disabled={saving} onClick={confirmDiscardIntent}>Descartar intencion</Button>}</div>
        </section>
      )}
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4" aria-busy={loading}>
        <StatCard label="Entradas historicas" value={statValue(view?.stats.global_total_entries)} detail="Unidades registradas" />
        <StatCard label="Salidas historicas" value={statValue(view?.stats.global_total_exits)} detail="Unidades registradas" />
        <StatCard label="Existencia actual" value={statValue(view?.stats.global_current_stock)} detail="Unidades en catalogo" />
        <StatCard label="Productos agotados" value={statValue(view?.stats.global_out_of_stock_products)} detail="Sin unidades disponibles" />
      </div>

      <section className="mt-5 overflow-hidden rounded-card border border-line bg-surface">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line p-4 sm:p-5">
          <div><h2 id="inventory-table-title" className="font-heading text-xl font-bold">Historial de movimientos</h2>
          <p className="mt-1 text-xs text-muted">Entradas y salidas persistidas con los datos del producto al momento de la operacion.</p></div>
          <Button variant="primary" onClick={openMovement} disabled={Boolean(pendingIntent)}>Registrar movimiento</Button>
        </div>

        <div className="grid gap-3 border-b border-line bg-surface-soft/60 p-4 sm:grid-cols-3 sm:p-5">
          <Field label="Buscar producto" value={search} onChange={(event) => updateFilter(setSearch, event.target.value)} placeholder="Nombre o SKU" />
          <label className="grid gap-2 text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">
            Motivo
            <select className={selectClass} value={reason} onChange={(event) => updateFilter(setReason, event.target.value)}>
              <option value="">Todos los motivos</option>
              <option value="compra">Compra</option>
              <option value="ajuste">Ajuste</option>
              <option value="venta">Venta</option>
            </select>
          </label>
          <label className="grid gap-2 text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">
            Tipo
            <select className={selectClass} value={movementType} onChange={(event) => updateFilter(setMovementType, event.target.value)}>
              <option value="">Entradas y salidas</option>
              <option value="entrada">Entrada</option>
              <option value="salida">Salida</option>
            </select>
          </label>
        </div>

        {error && (
          <div className="m-4 flex flex-wrap items-center justify-between gap-3 rounded-control border border-danger/25 bg-danger-soft px-4 py-3 text-sm text-danger" role="alert">
            <span>{error}</span>
            <Button size="sm" onClick={() => void loadView()} disabled={loading}>Reintentar</Button>
          </div>
        )}

        {error && view && <p className="mx-4 mb-3 text-xs font-semibold text-muted">Se muestran los resultados anteriores; no corresponden a los filtros actuales.</p>}
        {loading && view && <p className="mx-4 mb-3 text-xs font-semibold text-muted">Actualizando; se muestran temporalmente los resultados anteriores.</p>}

        <div className="relative overflow-x-auto" aria-label="Movimientos de inventario desplazables" aria-busy={loading} tabIndex={0}>
          {loading && <div className="absolute inset-x-0 top-0 z-10 h-0.5 overflow-hidden bg-brand-soft"><div className="h-full w-1/2 bg-brand" /></div>}
          {!view && loading ? (
            <div className="grid min-h-64 place-items-center"><Spinner className="size-6 text-brand" /></div>
          ) : view?.items.length ? (
            <table aria-labelledby="inventory-table-title" className="w-full min-w-[760px] text-left text-sm">
              <caption className="sr-only">Movimientos historicos de inventario</caption>
              <thead>
                <tr className="text-[0.625rem] tracking-wider text-muted uppercase">
                  <th className="border-b border-line px-5 py-3">Producto</th>
                  <th className="border-b border-line px-5 py-3">Movimiento</th>
                  <th className="border-b border-line px-5 py-3 text-right">Cantidad</th>
                  <th className="border-b border-line px-5 py-3">Referencia</th>
                  <th className="border-b border-line px-5 py-3">Fecha</th>
                </tr>
              </thead>
              <tbody>
                {view.items.map((movement) => (
                  <tr key={movement.id} className="hover:bg-surface-soft/60">
                    <td className="border-b border-line px-5 py-4"><strong className="block">{movement.product_name}</strong><span className="text-xs text-muted">{movement.sku}</span></td>
                    <td className="border-b border-line px-5 py-4"><div className="flex flex-wrap gap-2"><span className={`rounded-full px-2.5 py-1 text-xs font-bold ${movement.type === "entrada" ? "bg-brand-soft text-brand" : "bg-danger-soft text-danger"}`}>{movement.type === "entrada" ? "Entrada" : "Salida"}</span><span className={`rounded-full px-2.5 py-1 text-xs font-bold ${reasonClasses[movement.reason]}`}>{reasonLabels[movement.reason]}</span></div></td>
                    <td className={`border-b border-line px-5 py-4 text-right font-bold ${movement.type === "entrada" ? "text-brand" : "text-danger"}`}>{movement.type === "entrada" ? "+" : "-"}{movement.quantity}</td>
                    <td className="border-b border-line px-5 py-4">{movement.ticket_reference ? <span className="font-mono text-xs font-bold text-ink">{movement.ticket_reference}</span> : <span className="text-muted">Sin ticket</span>}</td>
                    <td className="border-b border-line px-5 py-4 text-xs whitespace-nowrap text-muted">{formatDateTime(movement.created_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : !loading && !error && (
            <EmptyState title={hasFilters ? "No hay movimientos con estos filtros" : "Aun no hay movimientos"} description={hasFilters ? "Cambia la busqueda o los filtros para ampliar los resultados." : "Los movimientos apareceran al registrar productos y ventas."} icon="↕" />
          )}
        </div>
      </section>

      <Modal open={movementOpen} onClose={closeMovement} closeDisabled={saving} title="Registrar movimiento" description="El stock mostrado es informativo; se validara nuevamente al guardar." size="sm" footer={<>{intentCanBeDiscarded && <Button variant="ghost" onClick={confirmDiscardIntent} disabled={saving}>Descartar intencion</Button>}<Button onClick={closeMovement} disabled={saving}>Cancelar</Button><Button form="inventory-movement-form" type="submit" variant="primary" loading={saving} disabled={!cleanupRequired && (productsLoading || Boolean(productsError) || !products.length)}>{cleanupRequired ? "Reintentar limpieza" : "Guardar movimiento"}</Button></>}>
        <form id="inventory-movement-form" className="grid gap-4" noValidate onSubmit={(event) => void saveMovement(event)}>
          {movementError && <div className="rounded-control border border-danger/25 bg-danger-soft px-4 py-3 text-sm text-danger" role="alert">{movementError}</div>}
          {productsError && <div className="flex flex-wrap items-center justify-between gap-3 rounded-control border border-danger/25 bg-danger-soft px-4 py-3 text-sm text-danger" role="alert"><span>{productsError}</span><Button size="sm" onClick={() => void loadMovementProducts()} disabled={productsLoading || saving}>Reintentar</Button></div>}
          <label className="grid gap-2 text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">
            Producto <span className="sr-only">obligatorio</span>
            <select data-autofocus className={selectClass} value={productId} onChange={(event) => { setProductId(event.target.value); setMovementError(""); }} required disabled={productsLoading || saving || Boolean(productsError)}>
              <option value="">{productsLoading ? "Cargando productos..." : products.length ? "Selecciona un producto" : "No hay productos disponibles"}</option>
              {products.map((product) => <option key={product.id} value={product.id}>{product.name} - {product.sku}</option>)}
            </select>
          </label>
          {productId && <p className="rounded-control bg-surface-soft px-3 py-2 text-sm text-muted" aria-live="polite">Stock actual: <strong className="text-ink">{products.find((product) => product.id === Number(productId))?.stock}</strong></p>}
          <label className="grid gap-2 text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">
            Operacion <span className="sr-only">obligatorio</span>
            <select className={selectClass} value={operation} onChange={(event) => setOperation(event.target.value as Operation)} required disabled={saving || productsLoading}>
              <option value="purchase">Compra</option>
              <option value="adjustment-in">Ajuste de entrada</option>
              <option value="adjustment-out">Ajuste de salida</option>
            </select>
          </label>
          <Field ref={quantityRef} label="Cantidad" value={quantity} onChange={(event) => { setQuantity(event.target.value); setMovementError(""); setQuantityError(""); }} inputMode="numeric" pattern="[0-9]+" required disabled={saving || productsLoading} hint="Numero entero mayor que cero." error={quantityError || undefined} />
        </form>
      </Modal>
      <Modal open={discardOpen} onClose={() => setDiscardOpen(false)} title="Descartar intencion" description="Descarta solo si revisaste el error definitivo. El inventario se recargara antes de permitir otra operacion." size="sm" footer={<><Button onClick={() => setDiscardOpen(false)}>Conservar</Button><Button variant="danger" onClick={() => void discardIntent()}>Descartar y recargar</Button></>}>
        {discardError && <div className="mb-3 rounded-control border border-danger/25 bg-danger-soft px-4 py-3 text-sm text-danger" role="alert">{discardError}</div>}
        <p className="text-sm text-muted">Esta accion elimina del dispositivo la recuperacion pendiente para este usuario.</p>
      </Modal>
    </main>
  );
}
