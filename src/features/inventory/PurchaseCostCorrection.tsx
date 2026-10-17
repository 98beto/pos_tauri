import { useRef, useState, type FormEvent } from "react";
import { confirmPurchaseCostCorrection, quotePurchaseCostCorrection } from "../../api/inventory";
import { Button, Field, Modal } from "../../components/ui";
import { errorMessage, isAppError, isSessionRequired } from "../../lib/app-error";
import { formatBaseQuantity, formatDateTime, formatEnteredQuantity, formatMoney } from "../../lib/format";
import type { InventoryMovement, PurchaseCostCorrectionQuote } from "../../types/inventory_movement";

type Props = {
  movement: InventoryMovement;
  onApplied: () => void | Promise<void>;
  onSessionRequired: () => void;
};

export function canCorrectPurchaseCost(movement: InventoryMovement) {
  return movement.origin === "manual"
    && movement.reason === "compra"
    && movement.type === "entrada"
    && movement.record_version === 2
    && movement.unit_price_cents !== null
    && movement.movement_cost_cents !== null
    && movement.entered_quantity !== null
    && movement.base_quantity !== null
    && movement.stock_before !== null
    && movement.stock_after !== null;
}

function cost(value: number | null) {
  return value === null ? "Desconocido" : formatMoney(value);
}

function enteredQuantity(quote: PurchaseCostCorrectionQuote) {
  return quote.presentation_mode
    ? formatEnteredQuantity(quote.entered_quantity, quote.presentation_mode, quote.capture_name, quote.base_unit)
    : formatBaseQuantity(quote.entered_quantity, quote.base_unit);
}

export function PurchaseCostCorrection({ movement, onApplied, onSessionRequired }: Props) {
  const [open, setOpen] = useState(false);
  const [newPrice, setNewPrice] = useState("");
  const [quote, setQuote] = useState<PurchaseCostCorrectionQuote>();
  const [error, setError] = useState("");
  const [priceError, setPriceError] = useState("");
  const [busy, setBusy] = useState(false);
  const [applied, setApplied] = useState(false);
  const request = useRef(0);

  if (!canCorrectPurchaseCost(movement)) return null;

  function close() {
    if (busy) return;
    request.current += 1;
    setOpen(false);
    setNewPrice("");
    setQuote(undefined);
    setError("");
    setPriceError("");
    setApplied(false);
  }

  function changePrice(value: string) {
    request.current += 1;
    setNewPrice(value);
    setQuote(undefined);
    setError("");
    setPriceError("");
    setApplied(false);
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy || applied) return;
    const currentRequest = ++request.current;
    setBusy(true);
    setError("");
    setPriceError("");
    try {
      if (!quote) {
        const next = await quotePurchaseCostCorrection({ movement_id: movement.id, new_unit_price: newPrice });
        if (request.current === currentRequest) setQuote(next);
        return;
      }
      await confirmPurchaseCostCorrection({
        movement_id: movement.id,
        new_unit_price: newPrice,
        quote_fingerprint: quote.quote_fingerprint,
      });
      if (request.current !== currentRequest) return;
      setApplied(true);
      try {
        await onApplied();
      } catch {
        setError("La correccion fue aplicada, pero no se pudo actualizar la vista. Cierra el dialogo y reintenta la consulta.");
      }
    } catch (value: unknown) {
      if (request.current !== currentRequest) return;
      if (isSessionRequired(value)) {
        setOpen(false);
        onSessionRequired();
      } else if (isAppError(value) && value.code === "PURCHASE_COST_CORRECTION_STALE") {
        setQuote(undefined);
        setError("El inventario cambio desde la cotizacion. Calcula nuevamente el impacto antes de confirmar.");
      } else {
        const message = errorMessage(value, quote
          ? "No se recibio la confirmacion. Puedes reintentar de forma segura con la misma cotizacion."
          : "No se pudo calcular el impacto de la correccion.");
        if (isAppError(value) && value.field === "new_unit_price") setPriceError(message);
        else setError(message);
      }
    } finally {
      if (request.current === currentRequest) setBusy(false);
    }
  }

  return <>
    <Button size="sm" onClick={() => setOpen(true)} aria-label={`Corregir precio de compra de ${movement.product_name_snapshot}`}>Corregir precio</Button>
    <Modal
      open={open}
      onClose={close}
      closeDisabled={busy}
      title="Corregir precio de compra"
      description="La cantidad, el stock, la fecha y los datos de la compra permaneceran sin cambios."
      size="md"
      footer={<>
        <Button onClick={close} disabled={busy}>{applied ? "Cerrar" : "Cancelar"}</Button>
        {!applied && <Button form={`purchase-correction-${movement.id}`} type="submit" variant="primary" loading={busy}>{quote ? "Confirmar correccion" : "Calcular impacto"}</Button>}
      </>}
    >
      <form id={`purchase-correction-${movement.id}`} className="grid gap-4" noValidate onSubmit={(event) => void submit(event)}>
        {error && <div className="rounded-control border border-danger/25 bg-danger-soft px-4 py-3 text-sm text-danger" role="alert">{error}</div>}
        {applied && <div className="rounded-control border border-success/30 bg-success-soft px-4 py-3 text-sm font-semibold text-ink" role="status">La correccion ya fue aplicada y el historial de costos quedo actualizado.</div>}
        <section className="rounded-control border border-line bg-surface-soft p-4" aria-label="Compra seleccionada">
          <strong className="block text-sm text-ink">{movement.product_name_snapshot}</strong>
          <span className="mt-1 block text-xs text-muted">{movement.presentation_name ?? "Unidad base"} · {formatDateTime(movement.created_at)}</span>
          <span className="mt-2 block text-sm">Precio registrado: <b>{formatMoney(movement.unit_price_cents!)}</b></span>
        </section>
        <Field data-autofocus label="Nuevo precio de compra" value={newPrice} onChange={(event) => changePrice(event.target.value)} inputMode="decimal" placeholder="0.00" required disabled={busy || applied} error={priceError || undefined} hint="Usa el mismo precio por presentacion o base de captura de la compra original." />
        {quote && <section className="grid gap-4" aria-labelledby={`correction-impact-${movement.id}`}>
          <div>
            <h3 id={`correction-impact-${movement.id}`} className="font-heading text-lg font-bold">Impacto cotizado</h3>
            <p className="mt-1 text-xs text-muted">Captura original: {enteredQuantity(quote)}; movimiento al inventario: {formatBaseQuantity(quote.base_quantity, quote.base_unit)}.</p>
          </div>
          <dl className="grid gap-3 sm:grid-cols-2">
            <div className="rounded-control border border-line p-3"><dt className="text-xs font-bold text-muted">Precio de compra</dt><dd className="mt-1 font-bold">{formatMoney(quote.previous_unit_price_cents)} → {formatMoney(quote.new_unit_price_cents)}</dd></div>
            <div className="rounded-control border border-line p-3"><dt className="text-xs font-bold text-muted">Costo de esta compra</dt><dd className="mt-1 font-bold">{formatMoney(quote.previous_movement_cost_cents)} → {formatMoney(quote.new_movement_cost_cents)}</dd></div>
            <div className="rounded-control border border-line p-3"><dt className="text-xs font-bold text-muted">Costo actual del producto</dt><dd className="mt-1 font-bold">{cost(quote.current_inventory_cost_cents)} → {cost(quote.corrected_inventory_cost_cents)}</dd></div>
            <div className="rounded-control border border-line p-3"><dt className="text-xs font-bold text-muted">Historial recalculado</dt><dd className="mt-1 font-bold">{quote.affected_movement_count} movimientos · {quote.affected_sale_count} ventas</dd></div>
          </dl>
          <p className="text-xs text-muted">Filas afectadas: {formatDateTime(quote.first_affected_at)} a {formatDateTime(quote.last_affected_at)}.</p>
          <div className="rounded-control border border-warning/35 bg-warning-soft px-4 py-3 text-sm text-ink" role="note"><strong>Reemplazo irreversible.</strong> Al confirmar, el precio anterior y los costos derivados seran reemplazados. No se modificaran cantidades, stock ni importes comerciales de ventas.</div>
        </section>}
      </form>
    </Modal>
  </>;
}
