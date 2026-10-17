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
};

const selectClass = "h-11 w-full rounded-control border border-line-strong bg-surface px-3 text-sm text-ink outline-none focus:border-brand focus:ring-3 focus:ring-brand/12";
const reasons: Record<InventoryMovementReason, string> = { compra: "Compra", venta: "Venta", ajuste: "Ajuste", valoracion: "Valoracion" };
const types: Record<InventoryMovementType, string> = { entrada: "Entrada", salida: "Salida", valoracion: "Valoracion" };
const units = { piece: "Pieza", gram: "Gramo", milliliter: "Mililitro", centimeter: "Centimetro" } as const;
const unitPriceUnits = { piece: "pieza", gram: "gramo", milliliter: "mililitro", centimeter: "centimetro" } as const;

function movementQuantity(movement: InventoryMovement) {
  if (movement.type === "valoracion") return "Sin cambio de stock";
  if (movement.entered_quantity !== null && movement.presentation_mode && movement.presentation_name && movement.base_unit) {
    const entered = formatEnteredQuantity(movement.entered_quantity, movement.presentation_mode, movement.presentation_name, movement.base_unit);
    const base = movement.base_quantity === null ? null : formatBaseQuantity(movement.base_quantity, movement.base_unit);
    return base && entered !== base ? `${entered} (${base} base)` : entered;
  }
  if (movement.base_quantity !== null && movement.base_unit) return formatBaseQuantity(movement.base_quantity, movement.base_unit);
  return String(movement.quantity);
}

function movementUnitPrice(movement: InventoryMovement) {
  if (movement.unit_price_cents === null) return "-";
  const price = formatMoney(movement.unit_price_cents);
  const hasPresentationSnapshot = movement.presentation_mode !== null && movement.presentation_base_quantity !== null;
  if (hasPresentationSnapshot && movement.presentation_mode === "fixed") {
    return `${price} por ${movement.presentation_name ?? "presentacion"}`;
  }
  if (movement.presentation_mode === "measured" && movement.presentation_base_quantity !== null && movement.base_unit) {
    return `${price} por ${formatBaseQuantity(movement.presentation_base_quantity, movement.base_unit)}`;
  }
  return movement.base_unit ? `${price} por ${unitPriceUnits[movement.base_unit]}` : price;
}

function PageControls({ page, total, pageSize, onPage }: { page: number; total: number; pageSize: number; onPage: (page: number) => void }) {
  const pages = Math.max(1, Math.ceil(total / pageSize));
  if (pages <= 1) return null;
  return <nav className="flex items-center justify-between gap-3 border-t border-line px-4 py-3" aria-label="Paginacion del detalle"><Button size="sm" disabled={page <= 1} onClick={() => onPage(page - 1)}>Anterior</Button><span className="text-xs font-semibold text-muted">Pagina {page} de {pages}</span><Button size="sm" disabled={page >= pages} onClick={() => onPage(page + 1)}>Siguiente</Button></nav>;
}

export function ProductInventoryDetail({ productId, onBack, onSessionRequired }: Props) {
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

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => titleRef.current?.focus());
    return () => window.cancelAnimationFrame(frame);
  }, []);

  async function load() {
    const request = ++requestRef.current;
    setLoading(true);
    setError("");
    try {
      const next = await getProductInventoryDetail(productId, {
        movement_search: search || undefined,
        reason: (reason || undefined) as InventoryMovementReason | undefined,
        type: (movementType || undefined) as InventoryMovementType | undefined,
        date_from: dateFrom || undefined,
        date_to: dateTo || undefined,
        page,
      });
      if (requestRef.current !== request) return;
      setDetail(next);
    } catch (value: unknown) {
      if (requestRef.current !== request) return;
      if (isSessionRequired(value)) onSessionRequired();
      else setError(errorMessage(value, "No se pudo consultar el inventario del producto."));
    } finally {
      if (requestRef.current === request) setLoading(false);
    }
  }

  useEffect(() => {
    const timeout = window.setTimeout(() => void load(), 250);
    return () => { window.clearTimeout(timeout); requestRef.current += 1; };
  }, [productId, search, reason, movementType, dateFrom, dateTo, page]);

  function filter(setter: (value: string) => void, value: string) {
    setPage(1);
    setter(value);
  }

  const product = detail?.product;
  const hasFilters = Boolean(search || reason || movementType || dateFrom || dateTo);
  return (
    <section aria-labelledby="product-inventory-title">
      <Button onClick={onBack} className="mb-4">Volver al inventario</Button>
      <header className="rounded-card border border-line bg-surface p-4 sm:p-5">
        <p className="text-xs font-bold tracking-wider text-brand uppercase">Detalle de inventario</p>
        <h1 ref={titleRef} id="product-inventory-title" tabIndex={-1} className="mt-1 font-heading text-2xl font-bold outline-none">{product?.name ?? "Cargando producto"}</h1>
        <p className="mt-1 font-mono text-xs text-muted">{product?.sku ?? ""}</p>
        <div className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-5" aria-busy={loading}>
          <StatCard label="Unidad base" value={product ? units[product.base_unit] : "-"} />
          <StatCard label="Stock actual" value={product ? formatBaseQuantity(product.stock, product.base_unit) : "-"} />
          <StatCard label="Costo promedio" value={product?.average_cost_cents == null ? "Desconocido" : formatMoney(product.average_cost_cents)} />
          <StatCard label="Valor a costo" value={product?.inventory_cost_cents == null ? "Desconocido" : formatMoney(product.inventory_cost_cents)} detail="Costo acumulado del inventario actual." />
          <StatCard label="Ultimo movimiento" value={detail ? detail.last_movement_at ? formatDateTime(detail.last_movement_at) : "Sin movimientos" : "-"} />
        </div>
      </header>

      <section className="mt-5 overflow-hidden rounded-card border border-line bg-surface">
        <div className="border-b border-line p-4 sm:p-5"><h2 className="font-heading text-xl font-bold">Movimientos del producto</h2><p className="mt-1 text-xs text-muted">Consulta costos, documentos y cantidades registradas.</p></div>
        <div className="grid gap-3 border-b border-line bg-surface-soft/60 p-4 sm:grid-cols-2 lg:grid-cols-5">
          <Field label="Buscar referencia" value={search} onChange={(event) => filter(setSearch, event.target.value)} placeholder="Proveedor, documento, nota o ticket" />
          <label className="grid gap-2 text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">Motivo<select className={selectClass} value={reason} onChange={(event) => filter(setReason, event.target.value)}><option value="">Todos</option><option value="compra">Compra</option><option value="venta">Venta</option><option value="ajuste">Ajuste</option><option value="valoracion">Valoracion</option></select></label>
          <label className="grid gap-2 text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">Tipo<select className={selectClass} value={movementType} onChange={(event) => filter(setMovementType, event.target.value)}><option value="">Todos</option><option value="entrada">Entrada</option><option value="salida">Salida</option><option value="valoracion">Valoracion</option></select></label>
          <Field label="Fecha inicial" type="date" value={dateFrom} onChange={(event) => filter(setDateFrom, event.target.value)} />
          <Field label="Fecha final" type="date" value={dateTo} onChange={(event) => filter(setDateTo, event.target.value)} />
        </div>
        {error && <div className="m-4 flex flex-wrap items-center justify-between gap-3 rounded-control border border-danger/25 bg-danger-soft px-4 py-3 text-sm text-danger" role="alert"><span>{error}</span><Button size="sm" onClick={() => void load()} disabled={loading}>Reintentar</Button></div>}
        {error && detail && <p className="mx-4 mb-3 text-xs font-semibold text-muted">Se muestran los resultados anteriores; no corresponden a los filtros actuales.</p>}
        {loading && detail && <p className="mx-4 mb-3 text-xs font-semibold text-muted">Actualizando; se muestran temporalmente los resultados anteriores.</p>}
        <div className="relative overflow-x-auto focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand" role="region" aria-label="Movimientos del producto desplazables" aria-busy={loading} tabIndex={0}>
          {!detail && loading ? <div className="grid min-h-64 place-items-center"><Spinner className="size-6 text-brand" /></div> : detail?.movements.length ? (
             <table className="w-full min-w-[1350px] text-left text-sm"><caption className="sr-only">Movimientos del inventario del producto</caption><thead><tr className="text-[0.625rem] tracking-wider text-muted uppercase"><th className="border-b border-line px-4 py-3">Fecha</th><th className="border-b border-line px-4 py-3">Tipo y motivo</th><th className="border-b border-line px-4 py-3">Presentacion</th><th className="border-b border-line px-4 py-3">Cantidad</th><th className="border-b border-line px-4 py-3 text-right">Precio / costo unitario</th><th className="border-b border-line px-4 py-3 text-right">Costo movimiento</th><th className="border-b border-line px-4 py-3 text-right">Costo acumulado</th><th className="border-b border-line px-4 py-3">Proveedor / documento</th><th className="border-b border-line px-4 py-3">Ticket / nota</th><th className="border-b border-line px-4 py-3"><span className="sr-only">Acciones</span></th></tr></thead>
              <tbody>{detail.movements.map((movement) => <tr key={movement.id} className="align-top hover:bg-surface-soft/60"><td className="border-b border-line px-4 py-4 text-xs whitespace-nowrap text-muted">{formatDateTime(movement.created_at)}</td><td className="border-b border-line px-4 py-4"><strong>{types[movement.type]}</strong><span className="block text-xs text-muted">{reasons[movement.reason]}</span></td><td className="border-b border-line px-4 py-4">{movement.presentation_name ?? "Unidad base"}</td><td className="border-b border-line px-4 py-4">{movementQuantity(movement)}</td><td className="border-b border-line px-4 py-4 text-right">{movementUnitPrice(movement)}</td><td className="border-b border-line px-4 py-4 text-right font-bold">{movement.movement_cost_cents == null ? <span className="text-warning">Desconocido</span> : formatMoney(movement.movement_cost_cents)}</td><td className="border-b border-line px-4 py-4 text-right">{movement.inventory_cost_after_cents == null ? <span className="text-warning">Desconocido</span> : formatMoney(movement.inventory_cost_after_cents)}</td><td className="border-b border-line px-4 py-4">{movement.supplier || movement.document_reference ? <>{movement.supplier ?? "Sin proveedor"}<span className="block text-xs text-muted">{movement.document_reference ?? "Sin documento"}</span></> : "-"}</td><td className="border-b border-line px-4 py-4">{movement.ticket_reference || movement.note ? <>{movement.ticket_reference ?? "Sin ticket"}<span className="block text-xs text-muted">{movement.note ?? "Sin nota"}</span></> : "-"}</td><td className="border-b border-line px-4 py-4"><PurchaseCostCorrection movement={movement} onApplied={load} onSessionRequired={onSessionRequired} /></td></tr>)}</tbody></table>
          ) : !loading && !error ? <EmptyState title={hasFilters ? "No hay movimientos con estos filtros" : "Este producto aun no tiene movimientos"} description={hasFilters ? "Cambia los filtros para ampliar los resultados." : "Puedes registrar una compra o ajuste desde Inventario."} icon="↕" /> : null}
        </div>
        {detail && <PageControls page={page} total={detail.total} pageSize={detail.page_size} onPage={setPage} />}
      </section>
    </section>
  );
}
