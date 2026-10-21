import { useEffect, useRef, useState } from "react";
import { getProductInventoryDetail } from "../../api/inventory";
import { Button, EmptyState, Field, Spinner, StatCard } from "../../components/ui";
import { errorMessage, isSessionRequired } from "../../lib/app-error";
import { formatBaseQuantity, formatDateTime, formatEnteredQuantity, formatMoney } from "../../lib/format";
import type { InventoryMovement, InventoryMovementReason, InventoryMovementType, ProductInventoryDetail as DetailData } from "../../types/inventory_movement";
import { PurchaseCostCorrection } from "./PurchaseCostCorrection";

type Props = {
  productId: number;
  onBack: () => void;
  onSessionRequired: () => void;
  onPurchase?: () => void;
  onStockCount?: () => void;
  actionsDisabled?: boolean;
};

const selectClass = "h-11 w-full rounded-control border border-line-strong bg-surface px-3 text-sm text-ink outline-none focus:border-brand focus:ring-3 focus:ring-brand/12";
const units = { piece: "Pieza", gram: "Gramo", milliliter: "Mililitro", centimeter: "Centimetro" } as const;

function movementQuantity(movement: InventoryMovement) {
  if (movement.entered_quantity !== null && movement.presentation_mode && movement.presentation_name && movement.base_unit) {
    const entered = formatEnteredQuantity(movement.entered_quantity, movement.presentation_mode, movement.presentation_name, movement.base_unit);
    const base = movement.base_quantity === null ? null : formatBaseQuantity(movement.base_quantity, movement.base_unit);
    return base && entered !== base ? `${entered} (${base} base)` : entered;
  }
  return movement.base_quantity !== null && movement.base_unit ? formatBaseQuantity(movement.base_quantity, movement.base_unit) : String(movement.quantity);
}

function purchasePrice(movement: InventoryMovement) {
  if (movement.purchase_unit_price_cents === null) return "Compra historica sin precio";
  const price = formatMoney(movement.purchase_unit_price_cents);
  if (movement.presentation_mode === "fixed") return `${price} por ${movement.presentation_name ?? "presentacion"}`;
  if (movement.presentation_mode === "measured" && movement.presentation_base_quantity && movement.base_unit) return `${price} por ${formatBaseQuantity(movement.presentation_base_quantity, movement.base_unit)}`;
  return `${price} por ${movement.base_unit ? units[movement.base_unit].toLowerCase() : "unidad base"}`;
}

function MovementDetail({ movement, onApplied, onSessionRequired }: { movement: InventoryMovement; onApplied: () => Promise<void>; onSessionRequired: () => void }) {
  const baseUnit = movement.base_unit;
  return <article className="rounded-control border border-line p-4">
    <div className="flex flex-wrap items-start justify-between gap-2"><div><strong>{movement.reason === "compra" ? "Compra" : movement.reason === "ajuste" ? "Ajuste de existencias" : "Venta"}</strong><span className="ml-2 text-xs font-semibold text-muted">{movement.type === "entrada" ? "Entrada" : "Salida"}</span></div><span className="text-xs text-muted">{formatDateTime(movement.created_at)}</span></div>
    <dl className="mt-3 grid gap-2 text-sm sm:grid-cols-2">
      {movement.reason === "compra" && <><div><dt className="text-xs font-bold text-muted">Cantidad / presentacion</dt><dd>{movementQuantity(movement)} · {movement.presentation_name ?? "Unidad base"}</dd></div><div><dt className="text-xs font-bold text-muted">Precio y base</dt><dd>{purchasePrice(movement)}</dd></div><div><dt className="text-xs font-bold text-muted">Importe</dt><dd>{movement.purchase_total_cents === null ? "No disponible en historia" : formatMoney(movement.purchase_total_cents)}</dd></div><div><dt className="text-xs font-bold text-muted">Proveedor / factura</dt><dd>{movement.supplier ?? "Sin proveedor historico"} · {movement.document_reference ?? "Sin factura historica"}</dd></div>{movement.note && <div><dt className="text-xs font-bold text-muted">Nota</dt><dd>{movement.note}</dd></div>}</>}
      {movement.reason === "ajuste" && <><div><dt className="text-xs font-bold text-muted">Stock anterior</dt><dd>{movement.stock_before !== null && baseUnit ? formatBaseQuantity(movement.stock_before, baseUnit) : "-"}</dd></div><div><dt className="text-xs font-bold text-muted">Stock contado</dt><dd>{movement.stock_after !== null && baseUnit ? formatBaseQuantity(movement.stock_after, baseUnit) : "-"}</dd></div><div><dt className="text-xs font-bold text-muted">Diferencia</dt><dd>{movement.type === "entrada" ? "+" : "-"}{movement.base_quantity !== null && baseUnit ? formatBaseQuantity(movement.base_quantity, baseUnit) : movement.quantity}</dd></div><div><dt className="text-xs font-bold text-muted">Motivo</dt><dd>{movement.note ?? "Sin motivo historico"}</dd></div></>}
      {movement.reason === "venta" && <><div><dt className="text-xs font-bold text-muted">Cantidad retirada</dt><dd>{movementQuantity(movement)}</dd></div><div><dt className="text-xs font-bold text-muted">Ticket relacionado</dt><dd>{movement.ticket_reference ?? "Sin referencia"}</dd></div></>}
    </dl>
    <div className="mt-3"><PurchaseCostCorrection movement={movement} onApplied={onApplied} onSessionRequired={onSessionRequired} /></div>
  </article>;
}

function PageControls({ page, total, pageSize, onPage }: { page: number; total: number; pageSize: number; onPage: (page: number) => void }) {
  const pages = Math.max(1, Math.ceil(total / pageSize));
  if (pages === 1) return null;
  return <nav className="flex items-center justify-between border-t border-line p-4" aria-label="Paginacion del detalle"><Button size="sm" disabled={page === 1} onClick={() => onPage(page - 1)}>Anterior</Button><span className="text-xs font-semibold text-muted">Pagina {page} de {pages}</span><Button size="sm" disabled={page === pages} onClick={() => onPage(page + 1)}>Siguiente</Button></nav>;
}

export function ProductInventoryDetail({ productId, onBack, onSessionRequired, onPurchase, onStockCount, actionsDisabled = false }: Props) {
  const [detail, setDetail] = useState<DetailData>();
  const [search, setSearch] = useState("");
  const [reason, setReason] = useState("");
  const [movementType, setMovementType] = useState("");
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const requestRef = useRef(0);
  const titleRef = useRef<HTMLHeadingElement>(null);

  useEffect(() => { const frame = window.requestAnimationFrame(() => titleRef.current?.focus()); return () => window.cancelAnimationFrame(frame); }, []);
  async function load() {
    const request = ++requestRef.current; setLoading(true); setError("");
    try {
      const next = await getProductInventoryDetail(productId, { movement_search: search || undefined, reason: (reason || undefined) as InventoryMovementReason | undefined, type: (movementType || undefined) as InventoryMovementType | undefined, date_from: dateFrom || undefined, date_to: dateTo || undefined, page });
      if (request === requestRef.current) setDetail(next);
    } catch (value) {
      if (request !== requestRef.current) return;
      if (isSessionRequired(value)) onSessionRequired(); else setError(errorMessage(value, "No se pudo consultar el inventario del producto."));
    } finally { if (request === requestRef.current) setLoading(false); }
  }
  useEffect(() => {
    setLoading(true); setError("");
    const timer = window.setTimeout(() => void load(), 250);
    return () => { window.clearTimeout(timer); requestRef.current += 1; };
  }, [productId, search, reason, movementType, dateFrom, dateTo, page]);
  function filter(setter: (value: string) => void, value: string) { setLoading(true); setError(""); setPage(1); setter(value); }

  const product = detail?.product;
  const hasFilters = Boolean(search || reason || movementType || dateFrom || dateTo);
  return <section aria-labelledby="product-inventory-title">
    <Button onClick={onBack} className="mb-4">Volver al inventario</Button>
    <header className="rounded-card border border-line bg-surface p-4 sm:p-5"><p className="text-xs font-bold tracking-wider text-brand uppercase">Detalle de inventario</p><h1 ref={titleRef} id="product-inventory-title" tabIndex={-1} className="mt-1 font-heading text-2xl font-bold outline-none">{product?.name ?? "Cargando producto"}</h1><p className="mt-1 font-mono text-xs text-muted">{product?.sku ?? ""}</p><div className="mt-4 grid gap-3 sm:grid-cols-3" aria-busy={loading}><StatCard label="Unidad base" value={product ? units[product.base_unit] : "-"} /><StatCard label="Stock actual" value={product ? formatBaseQuantity(product.stock, product.base_unit) : "-"} /><StatCard label="Ultimo movimiento" value={detail ? detail.last_movement_at ? formatDateTime(detail.last_movement_at) : "Sin movimientos" : "-"} /></div><div className="mt-4 flex flex-wrap gap-2"><Button variant="primary" onClick={onPurchase} disabled={!onPurchase || actionsDisabled}>Registrar compra</Button><Button onClick={onStockCount} disabled={!onStockCount || actionsDisabled}>Ajustar existencias</Button></div></header>
    <section className="mt-5 overflow-hidden rounded-card border border-line bg-surface" aria-busy={loading || undefined}><div className="border-b border-line p-4 sm:p-5"><h2 className="font-heading text-xl font-bold">Movimientos del producto</h2><p className="mt-1 text-xs text-muted">Historia fisica y documental de compras, ajustes y ventas.</p></div>
      <div className="grid gap-3 border-b border-line bg-surface-soft/60 p-4 sm:grid-cols-2 lg:grid-cols-5"><Field label="Buscar referencia" value={search} onChange={(event) => filter(setSearch, event.target.value)} /><label className="grid gap-2 text-[0.625rem] font-extrabold tracking-wider text-muted uppercase">Motivo<select className={selectClass} value={reason} onChange={(event) => filter(setReason, event.target.value)}><option value="">Todos</option><option value="compra">Compra</option><option value="ajuste">Ajuste</option><option value="venta">Venta</option></select></label><label className="grid gap-2 text-[0.625rem] font-extrabold tracking-wider text-muted uppercase">Direccion<select className={selectClass} value={movementType} onChange={(event) => filter(setMovementType, event.target.value)}><option value="">Todas</option><option value="entrada">Entrada</option><option value="salida">Salida</option></select></label><Field label="Fecha inicial" type="date" value={dateFrom} onChange={(event) => filter(setDateFrom, event.target.value)} /><Field label="Fecha final" type="date" value={dateTo} onChange={(event) => filter(setDateTo, event.target.value)} /></div>
      {error && <div className="m-4 flex items-center justify-between gap-3 rounded-control border border-danger/25 bg-danger-soft p-3 text-sm text-danger" role="alert"><span>{error}</span><Button size="sm" onClick={() => void load()} disabled={loading}>Reintentar</Button></div>}
      {error && detail && <p className="mx-4 mb-3 text-xs font-semibold text-muted">Se muestran los resultados anteriores; no corresponden a los filtros actuales.</p>}
      {loading && detail && <p className="mx-4 mb-3 text-xs font-semibold text-muted" role="status">Actualizando; se muestran temporalmente los resultados anteriores.</p>}
      {!detail && loading ? <div className="grid min-h-64 place-items-center"><Spinner className="size-6 text-brand" /></div> : detail?.movements.length ? <div className="grid gap-3 p-4 lg:grid-cols-2">{detail.movements.map((movement) => <MovementDetail key={movement.id} movement={movement} onApplied={load} onSessionRequired={onSessionRequired} />)}</div> : !loading && !error ? <EmptyState title={hasFilters ? "No hay movimientos con estos filtros" : "Este producto aun no tiene movimientos"} description={hasFilters ? "Cambia los filtros para ampliar los resultados." : "Registra una compra o un conteo desde este producto."} icon="↕" /> : null}
      {detail && !loading && !error && <PageControls page={page} total={detail.total} pageSize={detail.page_size} onPage={(nextPage) => { setLoading(true); setError(""); setPage(nextPage); }} />}
    </section>
  </section>;
}
