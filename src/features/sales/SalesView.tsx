import { useEffect, useRef, useState } from "react";
import { getSaleHistoryDetail, getSalesView } from "../../api/sales";
import { Button, EmptyState, Field, Modal, Spinner, StatCard } from "../../components/ui";
import { errorMessage, isSessionRequired } from "../../lib/app-error";
import { formatBaseQuantity, formatDateTime, formatEnteredQuantity, formatMoney, formatPricePerUnit } from "../../lib/format";
import type { PaymentMethod, SaleHistoryDetail, SalesView as SalesViewData } from "../../types/sale";

type SalesViewProps = {
  onSessionRequired: () => void;
};

const paymentLabels: Record<PaymentMethod, string> = {
  efectivo: "Efectivo",
  tarjeta: "Tarjeta",
  transferencia: "Transferencia",
};

const paymentClasses: Record<PaymentMethod, string> = {
  efectivo: "bg-brand-soft text-brand",
  tarjeta: "bg-info-soft text-info",
  transferencia: "bg-warning-soft text-warning",
};

const selectClass = "h-11 w-full rounded-control border border-line-strong bg-surface px-3 text-sm text-ink outline-none transition focus:border-brand focus:ring-3 focus:ring-brand/12";

function discountLabel(type: SaleHistoryDetail["discount_type"], value: number) {
  if (type === "percentage") {
    const percentage = `${Math.trunc(value / 100)}.${String(value % 100).padStart(2, "0")}`.replace(/\.00$/, "");
    return `Descuento ${percentage}%`;
  }
  return "Descuento fijo";
}

export function SalesView({ onSessionRequired }: SalesViewProps) {
  const [view, setView] = useState<SalesViewData>();
  const [search, setSearch] = useState("");
  const [paymentMethod, setPaymentMethod] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [detailOpen, setDetailOpen] = useState(false);
  const [detail, setDetail] = useState<SaleHistoryDetail>();
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState("");
  const [selectedSaleId, setSelectedSaleId] = useState<number>();
  const viewRequest = useRef(0);
  const detailRequest = useRef(0);
  const openingSale = useRef<number | undefined>(undefined);

  async function loadView() {
    const request = ++viewRequest.current;
    setLoading(true);
    setError("");
    try {
      const nextView = await getSalesView({
        search: search || undefined,
        payment_method: (paymentMethod || undefined) as PaymentMethod | undefined,
      });
      if (viewRequest.current !== request) return;
      setView(nextView);
    } catch (value: unknown) {
      if (viewRequest.current !== request) return;
      if (isSessionRequired(value)) onSessionRequired();
      else setError(errorMessage(value, "No se pudo consultar el historial de ventas."));
    } finally {
      if (viewRequest.current === request) setLoading(false);
    }
  }

  async function loadDetail(saleId: number) {
    const request = ++detailRequest.current;
    setDetailLoading(true);
    setDetailError("");
    try {
      const nextDetail = await getSaleHistoryDetail(saleId);
      if (detailRequest.current !== request) return;
      setDetail(nextDetail);
    } catch (value: unknown) {
      if (detailRequest.current !== request) return;
      if (isSessionRequired(value)) onSessionRequired();
      else setDetailError(errorMessage(value, "No se pudo cargar el detalle de la venta."));
    } finally {
      if (detailRequest.current === request) {
        openingSale.current = undefined;
        setDetailLoading(false);
      }
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
  }, [search, paymentMethod]);

  useEffect(() => () => {
    detailRequest.current += 1;
  }, []);

  function openDetail(saleId: number) {
    if (openingSale.current === saleId) return;
    openingSale.current = saleId;
    setSelectedSaleId(saleId);
    setDetail(undefined);
    setDetailOpen(true);
    void loadDetail(saleId);
  }

  function closeDetail() {
    detailRequest.current += 1;
    openingSale.current = undefined;
    setDetailLoading(false);
    setDetailOpen(false);
    setDetail(undefined);
    setDetailError("");
    setSelectedSaleId(undefined);
  }

  const hasFilters = Boolean(search || paymentMethod);
  const statsDetail = loading
    ? "Pendiente de actualizacion"
    : error
      ? "No disponible por el error actual"
      : hasFilters
        ? "Segun filtros y resultados actuales"
        : "Segun los resultados actuales";
  const statValue = (value: number | undefined, money = false) =>
    loading || error || value === undefined ? "-" : money ? formatMoney(value) : value;

  function updateSearch(value: string) {
    setLoading(true);
    setError("");
    setSearch(value);
  }

  function updatePaymentMethod(value: string) {
    setLoading(true);
    setError("");
    setPaymentMethod(value);
  }

  return (
    <main id="main-content" tabIndex={-1} className="p-4 outline-none sm:p-6 min-[900px]:p-8">
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4" aria-busy={loading}>
        <StatCard label="Ventas en resultados" value={statValue(view?.stats.sale_count)} detail={statsDetail} />
        <StatCard label="Total en resultados" value={statValue(view?.stats.total_sold, true)} detail={statsDetail} />
        <StatCard label="Promedio en resultados" value={statValue(view?.stats.average_sale, true)} detail={statsDetail} />
        <StatCard label="Lineas en resultados" value={statValue(view?.stats.lines_sold)} detail={statsDetail} />
      </div>

      <section className="mt-5 overflow-hidden rounded-card border border-line bg-surface">
        <div className="border-b border-line p-4 sm:p-5">
          <h2 id="sales-table-title" className="font-heading text-xl font-bold">Historial de tickets</h2>
          <p className="mt-1 text-xs text-muted">Ventas persistidas y disponibles para consulta.</p>
        </div>
        <div className="grid gap-3 border-b border-line bg-surface-soft/60 p-4 sm:grid-cols-[minmax(0,1fr)_280px] sm:p-5">
          <Field label="Buscar ticket" value={search} onChange={(event) => updateSearch(event.target.value)} placeholder="POS-00000001" />
          <label className="grid gap-2 text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">
            Metodo de pago
            <select className={selectClass} value={paymentMethod} onChange={(event) => updatePaymentMethod(event.target.value)}>
              <option value="">Todos los metodos</option>
              <option value="efectivo">Efectivo</option>
              <option value="tarjeta">Tarjeta</option>
              <option value="transferencia">Transferencia</option>
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

        <div className="relative overflow-x-auto focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand" role="region" aria-label="Historial de tickets desplazable" aria-busy={loading} tabIndex={0}>
          {loading && <div className="absolute inset-x-0 top-0 z-10 h-0.5 overflow-hidden bg-brand-soft"><div className="h-full w-1/2 bg-brand" /></div>}
          {!view && loading ? (
            <div className="grid min-h-64 place-items-center"><Spinner className="size-6 text-brand" /></div>
          ) : view?.items.length ? (
            <table aria-labelledby="sales-table-title" className="w-full min-w-[760px] text-left text-sm">
              <caption className="sr-only">Tickets del historial de ventas</caption>
               <thead><tr className="text-[0.625rem] tracking-wider text-muted uppercase"><th className="border-b border-line px-5 py-3">Ticket</th><th className="border-b border-line px-5 py-3">Metodo</th><th className="border-b border-line px-5 py-3 text-right">Lineas</th><th className="border-b border-line px-5 py-3 text-right">Total</th><th className="border-b border-line px-5 py-3">Fecha</th><th className="border-b border-line px-5 py-3"><span className="sr-only">Acciones</span></th></tr></thead>
              <tbody>
                {view.items.map((sale) => (
                  <tr key={sale.id} className="hover:bg-surface-soft/60">
                    <td className="border-b border-line px-5 py-4"><strong className="font-mono text-xs">{sale.ticket_number}</strong><span className="mt-1 block text-xs text-muted">{sale.user_name}</span></td>
                    <td className="border-b border-line px-5 py-4"><span className={`rounded-full px-2.5 py-1 text-xs font-bold ${paymentClasses[sale.payment_method]}`}>{paymentLabels[sale.payment_method]}</span></td>
                    <td className="border-b border-line px-5 py-4 text-right font-semibold">{sale.line_count}</td>
                    <td className="border-b border-line px-5 py-4 text-right font-bold">{formatMoney(sale.total)}</td>
                    <td className="border-b border-line px-5 py-4 text-xs whitespace-nowrap text-muted">{formatDateTime(sale.sale_date)}</td>
                    <td className="border-b border-line px-5 py-4 text-right"><Button size="sm" variant="ghost" aria-label={`Ver detalle de ${sale.ticket_number}`} disabled={detailLoading && selectedSaleId === sale.id} onClick={() => openDetail(sale.id)}>Ver detalle</Button></td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : !loading && !error && (
            <EmptyState title={hasFilters ? "No hay tickets con estos filtros" : "Aun no hay ventas"} description={hasFilters ? "Cambia el ticket o el metodo de pago para ampliar los resultados." : "Los tickets apareceran despues de completar el primer cobro."} icon="#" />
          )}
        </div>
      </section>

      <Modal open={detailOpen} onClose={closeDetail} title="Detalle de venta" description={detail ? `${detail.ticket_number} · ${formatDateTime(detail.sale_date)}` : "Consultando el ticket guardado en SQLite."} size="lg" footer={<Button onClick={closeDetail}>Cerrar</Button>}>
        {detailLoading ? (
          <div className="grid min-h-64 place-items-center" role="status"><div className="text-center text-sm font-semibold text-muted"><Spinner className="mx-auto mb-3 size-6 text-brand" />Cargando detalle...</div></div>
        ) : detailError ? (
          <div className="grid min-h-64 place-items-center text-center" role="alert"><div className="max-w-sm"><strong className="block text-sm">No se pudo mostrar este ticket</strong><p className="mt-2 text-xs leading-5 text-danger">{detailError}</p><Button className="mt-4" onClick={() => selectedSaleId !== undefined && void loadDetail(selectedSaleId)}>Reintentar</Button></div></div>
        ) : detail ? (
          <div>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
              <div className="rounded-card bg-brand-soft p-4"><span className="text-[0.625rem] font-bold tracking-wider text-brand uppercase">Total</span><strong className="mt-2 block font-heading text-2xl text-brand-dark">{formatMoney(detail.total)}</strong></div>
              <div className="rounded-card border border-line p-4"><span className="text-[0.625rem] font-bold tracking-wider text-muted uppercase">Metodo</span><strong className="mt-2 block text-sm">{paymentLabels[detail.payment_method]}</strong></div>
                <div className="rounded-card border border-line p-4"><span className="text-[0.625rem] font-bold tracking-wider text-muted uppercase">Lineas</span><strong className="mt-2 block text-sm">{detail.line_count}</strong></div>
              <div className="rounded-card border border-line p-4"><span className="text-[0.625rem] font-bold tracking-wider text-muted uppercase">Atendio</span><strong className="mt-2 block text-sm">{detail.user_name}</strong><span className="mt-1 block truncate text-xs text-muted">{detail.user_email}</span></div>
            </div>

             <div className="mt-5 overflow-x-auto focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand" role="region" aria-label="Lineas del ticket desplazables" tabIndex={0}>
               <table className="w-full min-w-[680px] text-left text-sm">
                 <caption className="sr-only">Lineas historicas del ticket {detail.ticket_number}</caption>
                 <thead><tr className="text-[0.625rem] tracking-wider text-muted uppercase"><th className="border-b border-line px-3 py-3">Producto</th><th className="border-b border-line px-3 py-3">Presentacion</th><th className="border-b border-line px-3 py-3 text-right">Cantidad</th><th className="border-b border-line px-3 py-3 text-right">Precio al cliente</th><th className="border-b border-line px-3 py-3 text-right">Total de linea</th></tr></thead>
                 <tbody>{detail.lines.map((line, index) => {
                   const hasPresentationSnapshot = line.presentation_name !== null && line.presentation_mode !== null && line.base_unit !== null;
                   const enteredQuantity = hasPresentationSnapshot
                     ? formatEnteredQuantity(line.entered_quantity, line.presentation_mode!, line.presentation_name!, line.base_unit!)
                     : `${line.quantity} ${line.quantity === 1 ? "unidad" : "unidades"}`;
                    const showBaseQuantity = hasPresentationSnapshot && line.presentation_mode === "fixed" && line.base_consumed !== line.entered_quantity;
                     const hasMeasuredPriceBase = Number.isSafeInteger(line.presentation_base_quantity) && line.presentation_base_quantity! > 0;
                     const unitPrice = hasPresentationSnapshot && line.presentation_mode === "measured" && hasMeasuredPriceBase
                        ? formatPricePerUnit(line.unit_price, line.base_unit!, line.presentation_base_quantity!)
                      : hasPresentationSnapshot && line.presentation_mode === "measured"
                        ? `${formatMoney(line.unit_price)} · base no disponible`
                      : line.presentation_name
                        ? `${formatMoney(line.unit_price)} por ${line.presentation_name}`
                        : formatMoney(line.unit_price);

                     const catalogPrice = line.catalog_unit_price !== null && line.catalog_unit_price !== line.unit_price
                       ? hasPresentationSnapshot && line.presentation_mode === "measured" && hasMeasuredPriceBase
                         ? formatPricePerUnit(line.catalog_unit_price, line.base_unit!, line.presentation_base_quantity!)
                         : hasPresentationSnapshot && line.presentation_mode === "measured"
                           ? `${formatMoney(line.catalog_unit_price)} · base no disponible`
                         : line.presentation_name
                           ? `${formatMoney(line.catalog_unit_price)} por ${line.presentation_name}`
                           : formatMoney(line.catalog_unit_price)
                      : null;

                     return <tr key={line.presentation_id ?? `legacy-line-${index}`}><td className="border-b border-line px-3 py-4"><strong className="block">{line.product_name}</strong><span className="text-xs text-muted">{line.sku}</span></td><td className="border-b border-line px-3 py-4"><strong className="block text-sm">{line.presentation_name ?? "Sin presentacion registrada"}</strong>{line.presentation_barcode && <span className="text-xs text-muted">{line.presentation_barcode}</span>}</td><td className="border-b border-line px-3 py-4 text-right"><span className="block font-semibold">{enteredQuantity}</span>{showBaseQuantity && <span className="text-xs text-muted">Base: {formatBaseQuantity(line.base_consumed, line.base_unit!)}</span>}</td><td className="border-b border-line px-3 py-4 text-right"><span className="block">Aplicado: {unitPrice}</span>{catalogPrice && <span className="mt-1 block text-xs text-muted">Catalogo: {catalogPrice}</span>}</td><td className="border-b border-line px-3 py-4 text-right font-bold">{line.subtotal === null ? "No representable" : formatMoney(line.subtotal)}</td></tr>;
                 })}</tbody>
               </table>
             </div>

             <dl className="ml-auto mt-5 grid max-w-sm gap-3 rounded-card border border-line p-4 text-sm">
               <div className="flex justify-between gap-4"><dt className="text-muted">Subtotal</dt><dd className="font-semibold">{formatMoney(detail.subtotal)}</dd></div>
               {detail.discount_type !== "none" && <div className="flex justify-between gap-4 text-danger"><dt>{discountLabel(detail.discount_type, detail.discount_value)}</dt><dd className="font-semibold">-{formatMoney(detail.discount_amount)}</dd></div>}
               <div className="flex justify-between gap-4 border-t border-line pt-3"><dt className="font-bold">Total</dt><dd className="font-bold text-brand">{formatMoney(detail.total)}</dd></div>
               {!detail.line_totals_verifiable && <p className="text-xs text-muted">El subtotal historico se conserva; las lineas legadas no permiten verificar su suma.</p>}
             </dl>

            {detail.payment_method === "efectivo" && (
              <dl className="ml-auto mt-5 grid max-w-sm gap-3 rounded-card bg-surface-soft p-4 text-sm">
                {detail.cash_received !== null && <div className="flex justify-between gap-4"><dt className="text-muted">Efectivo recibido</dt><dd className="font-semibold">{formatMoney(detail.cash_received)}</dd></div>}
                {detail.change_amount !== null && <div className="flex justify-between gap-4 border-t border-line pt-3"><dt className="font-bold">Cambio</dt><dd className="font-bold text-brand">{formatMoney(detail.change_amount)}</dd></div>}
              </dl>
            )}
          </div>
        ) : null}
      </Modal>
    </main>
  );
}
