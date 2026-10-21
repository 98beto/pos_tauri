import { useEffect, useRef, useState, type FormEvent } from "react";
import { confirmPurchasePriceCorrection, quotePurchasePriceCorrection } from "../../api/inventory";
import { Button, Field, Modal } from "../../components/ui";
import { errorMessage, isAppError, isSessionRequired } from "../../lib/app-error";
import { formatBaseQuantity, formatDateTime, formatEnteredQuantity, formatMoney } from "../../lib/format";
import type { InventoryMovement, PurchasePriceCorrectionQuote } from "../../types/inventory_movement";

type Props = { movement: InventoryMovement; onApplied: () => void | Promise<void>; onSessionRequired: () => void };

export function canCorrectPurchaseCost(movement: InventoryMovement) {
  return movement.origin === "manual" && movement.reason === "compra" && movement.type === "entrada" && movement.record_version === 3 && movement.purchase_unit_price_cents !== null && movement.purchase_total_cents !== null && movement.entered_quantity !== null && movement.base_quantity !== null;
}

function enteredQuantity(quote: PurchasePriceCorrectionQuote) {
  return quote.presentation_mode ? formatEnteredQuantity(quote.entered_quantity, quote.presentation_mode, quote.capture_name, quote.base_unit) : formatBaseQuantity(quote.entered_quantity, quote.base_unit);
}

export function PurchaseCostCorrection({ movement, onApplied, onSessionRequired }: Props) {
  const [open, setOpen] = useState(false);
  const [newPrice, setNewPrice] = useState("");
  const [quote, setQuote] = useState<PurchasePriceCorrectionQuote>();
  const [error, setError] = useState("");
  const [priceError, setPriceError] = useState("");
  const [busy, setBusy] = useState(false);
  const [applied, setApplied] = useState(false);
  const request = useRef(0);
  const generation = useRef(0);
  const onAppliedRef = useRef(onApplied);
  const onSessionRequiredRef = useRef(onSessionRequired);
  onAppliedRef.current = onApplied;
  onSessionRequiredRef.current = onSessionRequired;
  useEffect(() => {
    const current = ++generation.current;
    request.current += 1;
    setOpen(false); setNewPrice(""); setQuote(undefined); setError(""); setPriceError(""); setBusy(false); setApplied(false);
    return () => { if (generation.current === current) generation.current += 1; request.current += 1; };
  }, [movement.id, movement.updated_at]);
  if (!canCorrectPurchaseCost(movement)) return null;

  function close() { if (!busy) { request.current += 1; setOpen(false); setNewPrice(""); setQuote(undefined); setError(""); setPriceError(""); setApplied(false); } }
  function changePrice(value: string) { request.current += 1; setNewPrice(value); setQuote(undefined); setError(""); setPriceError(""); }
  async function submit(event: FormEvent) {
    event.preventDefault(); if (busy || applied) return;
    const current = ++request.current; const ownerGeneration = generation.current; setBusy(true); setError(""); setPriceError("");
    const isCurrent = () => generation.current === ownerGeneration && request.current === current;
    try {
      if (!quote) { const next = await quotePurchasePriceCorrection({ movement_id: movement.id, new_purchase_unit_price: newPrice }); if (isCurrent()) setQuote(next); return; }
      await confirmPurchasePriceCorrection({ movement_id: movement.id, new_purchase_unit_price: newPrice, quote_fingerprint: quote.quote_fingerprint });
      if (!isCurrent()) return;
      setApplied(true);
      try { await onAppliedRef.current(); } catch { if (isCurrent()) setError("La correccion fue aplicada, pero no se pudo actualizar la vista."); }
    } catch (value) {
      if (!isCurrent()) return;
      if (isSessionRequired(value)) { setOpen(false); onSessionRequiredRef.current(); }
      else if (isAppError(value) && value.code === "PURCHASE_PRICE_CORRECTION_CHANGED") { setQuote(undefined); setError("La compra cambio desde la cotizacion. Calcula nuevamente el importe."); }
      else { const message = errorMessage(value, quote ? "No se recibio la confirmacion. Puedes reintentar con la misma cotizacion." : "No se pudo cotizar el nuevo importe."); if (isAppError(value) && ["new_purchase_unit_price", "new_unit_price"].includes(value.field ?? "")) { setPriceError(message); window.requestAnimationFrame(() => document.querySelector<HTMLInputElement>(`#purchase-correction-${movement.id} [aria-invalid="true"]`)?.focus()); } else setError(message); }
    } finally { if (isCurrent()) setBusy(false); }
  }

  return <><Button size="sm" onClick={() => setOpen(true)} aria-label={`Corregir precio de compra de ${movement.product_name_snapshot}`}>Corregir precio</Button><Modal open={open} onClose={close} closeDisabled={busy} title="Corregir precio de compra" description="Solo cambiaran el precio y el importe documental de esta compra." size="md" footer={<><Button onClick={close} disabled={busy}>{applied ? "Cerrar" : "Cancelar"}</Button>{!applied && <Button form={`purchase-correction-${movement.id}`} type="submit" variant="primary" loading={busy}>{quote ? "Confirmar correccion" : "Calcular importe"}</Button>}</>}>
    <form id={`purchase-correction-${movement.id}`} className="grid gap-4" noValidate onSubmit={(event) => void submit(event)}>
      {error && <div className="rounded-control border border-danger/25 bg-danger-soft p-3 text-sm text-danger" role="alert">{error}</div>}
      {applied && <div className="rounded-control border border-success/30 bg-success-soft p-3 text-sm font-semibold" role="status">El precio y el importe de la compra fueron actualizados. No se modificaron stock, ventas ni otros movimientos.</div>}
      <section className="rounded-control border border-line bg-surface-soft p-4" aria-label="Compra seleccionada"><strong>{movement.product_name_snapshot}</strong><span className="mt-1 block text-xs text-muted">{movement.presentation_name ?? "Unidad base"} · {formatDateTime(movement.created_at)}</span><span className="mt-2 block text-sm">Precio registrado: <b>{formatMoney(movement.purchase_unit_price_cents!)}</b></span></section>
      <Field data-autofocus label="Nuevo precio de compra" value={newPrice} onChange={(event) => changePrice(event.target.value)} inputMode="decimal" placeholder="0.00" required disabled={busy || applied} error={priceError || undefined} hint="Usa la misma base de precio de la compra original." />
      {quote && <section className="grid gap-4" aria-labelledby={`correction-summary-${movement.id}`}><div><h3 id={`correction-summary-${movement.id}`} className="font-heading text-lg font-bold">Resumen por confirmar</h3><p className="mt-1 text-xs text-muted">Captura original: {enteredQuantity(quote)}; cantidad base: {formatBaseQuantity(quote.base_quantity, quote.base_unit)}.</p></div><dl className="grid gap-3 sm:grid-cols-2"><div className="rounded-control border border-line p-3"><dt className="text-xs font-bold text-muted">Precio de compra</dt><dd className="mt-1 font-bold">{formatMoney(quote.previous_purchase_unit_price_cents)} → {formatMoney(quote.new_purchase_unit_price_cents)}</dd></div><div className="rounded-control border border-line p-3"><dt className="text-xs font-bold text-muted">Importe de esta compra</dt><dd className="mt-1 font-bold">{formatMoney(quote.previous_purchase_total_cents)} → {formatMoney(quote.new_purchase_total_cents)}</dd></div></dl><div className="rounded-control border border-warning/35 bg-warning-soft p-3 text-sm" role="note">No se recalculan ventas, stock ni movimientos posteriores.</div></section>}
    </form>
  </Modal></>;
}
