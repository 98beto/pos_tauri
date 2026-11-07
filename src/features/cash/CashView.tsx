import { useEffect, useRef, useState, type FormEvent } from "react";
import { getCashState, getCashSessionDetail, listCashSessions } from "../../api/cash";
import { Button, EmptyState, Field, Modal, Spinner } from "../../components/ui";
import { errorMessage, isAppError, isSessionRequired } from "../../lib/app-error";
import { formatDateTime } from "../../lib/format";
import type { CashSession, CashSessionDetail } from "../../types/cash";
import type { User } from "../../types/user";
import { clearCashIntent, formatCashAmount as formatMoney, parseCashAmount, readCashIntent, saveCashIntent, submitCashIntent, type CashAction, type CashIntent } from "./cash-intent";

type Props = { user: User; onNotice: (notice: { tone: "success" | "error" | "info"; message: string }) => void; onSessionRequired: () => void };
const labels: Record<CashAction, string> = { open: "Abrir caja", income: "Registrar ingreso", withdraw: "Registrar retiro", close: "Cerrar caja" };
const money = (amount: number | null) => amount === null ? "No registrado" : formatMoney(amount);

function Summary({ session: s }: { session: CashSession }) {
  const values: [string, number | null][] = [["Fondo de apertura", s.opening_amount], ["Ventas en efectivo", s.breakdown.cash_sales], ["Tarjeta", s.breakdown.card_sales], ["Transferencia", s.breakdown.transfer_sales], ["Ingresos", s.breakdown.income], ["Retiros", s.breakdown.withdrawals], ["Esperado", s.expected_amount], ["Contado", s.counted_amount], ["Diferencia", s.difference]];
  return <div><h3 className="font-bold">Caja #{s.id} · {s.historical ? "Historica · Cerrada" : s.status === "open" ? "Abierta" : "Cerrada"}</h3><p className="mt-2 text-sm">{s.historical ? "Primera venta" : "Apertura"}: {formatDateTime(s.opened_at)} · {s.opening_operator ?? "No registrado"}</p><p className="text-sm">{s.historical ? "Ultima venta" : "Cierre"}: {s.closed_at ? formatDateTime(s.closed_at) : "No registrado"} · {s.closing_operator ?? "No registrado"}</p><dl className="mt-4 grid gap-3 sm:grid-cols-3">{values.map(([label, value]) => <div key={label} className="rounded-control border border-line p-3"><dt className="text-sm text-muted">{label}</dt><dd className="font-bold">{money(value)}</dd></div>)}</dl></div>;
}

export function CashView({ user, onNotice, onSessionRequired }: Props) {
  const [current, setCurrent] = useState<CashSession | null>();
  const [sessions, setSessions] = useState<CashSession[]>([]);
  const [detail, setDetail] = useState<CashSessionDetail>();
  const [detailOpen, setDetailOpen] = useState(false);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState("");
  const [selectedId, setSelectedId] = useState<number>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [formError, setFormError] = useState("");
  const [action, setAction] = useState<CashAction>();
  const [formSessionId, setFormSessionId] = useState<number>();
  const formTarget = useRef<number | undefined>(undefined);
  const [amount, setAmount] = useState("");
  const [reason, setReason] = useState("");
  const [pending, setPending] = useState<CashIntent>();
  const [storageError, setStorageError] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmation, setConfirmation] = useState<CashIntent>();
  const request = useRef(0);
  const detailRequest = useRef(0);
  const lifetime = useRef(0);
  const inFlight = useRef(false);
  const pendingRef = useRef<CashIntent | undefined>(undefined);

  function report(value: unknown, fallback: string, setter = setError) {
    setter(errorMessage(value, fallback));
    if (isSessionRequired(value)) onSessionRequired();
  }
  function readStored() {
    try { const saved = readCashIntent(user.id); setPending(saved); pendingRef.current = saved; setStorageError(""); }
    catch { setStorageError("No se pudo leer la intencion local. No se enviaran operaciones hasta recuperar el almacenamiento."); }
  }
  async function refresh() {
    const generation = lifetime.current;
    const id = ++request.current;
    setLoading(true); setError(""); setCurrent(undefined);
    try {
      const [state, history] = await Promise.all([getCashState(), listCashSessions()]);
      if (generation !== lifetime.current || id !== request.current) return;
      if (formTarget.current !== undefined && !pendingRef.current && (state?.id !== formTarget.current || state.status !== "open")) {
        setAction(undefined); setConfirmation(undefined); setAmount(""); setReason("");
        formTarget.current = undefined; setFormSessionId(undefined);
        setFormError("La caja del formulario cambio o se cerro. Abre un formulario nuevo y captura de nuevo los datos para la caja actual.");
      }
      setCurrent(state); setSessions(history);
    } catch (value) { if (generation === lifetime.current && id === request.current) report(value, "No se pudo consultar la caja."); }
    finally { if (generation === lifetime.current && id === request.current) setLoading(false); }
  }
  useEffect(() => {
    ++lifetime.current; inFlight.current = false; setBusy(false);
    setAction(undefined); setConfirmation(undefined); setDetailOpen(false); setDetail(undefined); setSessions([]); setFormError("");
    setPending(undefined); pendingRef.current = undefined;
    formTarget.current = undefined; setFormSessionId(undefined);
    readStored(); void refresh();
    const focus = () => { if (!inFlight.current) void refresh(); };
    window.addEventListener("focus", focus);
    return () => { ++lifetime.current; ++request.current; ++detailRequest.current; window.removeEventListener("focus", focus); };
  }, [user.id]);

  async function showDetail(id: number) {
    const generation = lifetime.current;
    const req = ++detailRequest.current;
    setSelectedId(id); setDetail(undefined); setDetailError(""); setDetailOpen(true); setDetailLoading(true);
    try { const result = await getCashSessionDetail(id); if (generation === lifetime.current && req === detailRequest.current) setDetail(result); }
    catch (value) { if (generation === lifetime.current && req === detailRequest.current) report(value, "No se pudo consultar el detalle.", setDetailError); }
    finally { if (generation === lifetime.current && req === detailRequest.current) setDetailLoading(false); }
  }
  function start(kind: CashAction) {
    if (pendingRef.current || inFlight.current || storageError || loading || error) return;
    if (kind !== "open" && current?.status !== "open") return;
    formTarget.current = kind === "open" ? undefined : current!.id;
    setFormSessionId(formTarget.current); setAction(kind); setAmount(""); setReason(""); setFormError("");
  }
  async function submit(intent: CashIntent) {
    if (inFlight.current || intent.userId !== user.id) return;
    const generation = lifetime.current;
    try { saveCashIntent(intent); } catch { setFormError("No se pudo proteger la operacion localmente. No se envio; recupera el almacenamiento y vuelve a intentar."); return; }
    inFlight.current = true; setBusy(true); pendingRef.current = intent; setPending(intent); setFormError("");
    try {
      const result = await submitCashIntent(intent);
      if (generation !== lifetime.current) return;
      // Apply the confirmed response before any fallible reload/cleanup.
      ++request.current; ++detailRequest.current;
      setDetail(result); setDetailOpen(true); setDetailLoading(false); setDetailError(""); setSelectedId(result.session.id);
      setSessions((old) => [result.session, ...old.filter((s) => s.id !== result.session.id)]);
      setCurrent(result.session.status === "open" ? result.session : null);
      setAction(undefined); setConfirmation(undefined); setAmount(""); setReason("");
      formTarget.current = undefined; setFormSessionId(undefined);
      try { clearCashIntent(intent); setPending(undefined); pendingRef.current = undefined; }
      catch { setStorageError("Operacion confirmada. No se pudo limpiar la intencion local; reintentar recupera la misma operacion sin duplicarla."); }
      onNotice({ tone: "success", message: "Operacion de caja confirmada." });
      await refresh();
    } catch (value) {
      if (generation !== lifetime.current) return;
      // Only explicit backend rejections prove the operation was not applied.
      if (isAppError(value) && ["validation", "conflict", "not_found"].includes(value.category)) {
        try { clearCashIntent(intent); setPending(undefined); pendingRef.current = undefined; setConfirmation(undefined); }
        catch { setStorageError("No se pudo limpiar la intencion rechazada. Reintenta con los mismos datos."); }
      }
      report(value, "Respuesta no confirmada. Conservamos los datos y el token; reintenta la misma operacion.", setFormError);
      await refresh();
    } finally { if (generation === lifetime.current) { inFlight.current = false; setBusy(false); } }
  }
  function prepare(event: FormEvent) {
    event.preventDefault(); if (!action || busy || pendingRef.current || loading || error || storageError) return;
    try {
      const cents = parseCashAmount(amount);
      if ((action === "income" || action === "withdraw") && cents <= 0) throw new Error("El importe debe ser mayor que cero.");
      const normalized = reason.trim();
      if ((action === "income" || action === "withdraw") && (!normalized || [...normalized].length > 500)) throw new Error("Captura un motivo de 1 a 500 caracteres.");
      if (action !== "open" && (current?.status !== "open" || current.id !== formTarget.current)) throw new Error("La caja del formulario cambio o se cerro. Abre un formulario nuevo.");
      if (action === "open" && current) throw new Error("Ya hay una caja abierta para todos los operadores.");
      const intent: CashIntent = { version: 1, userId: user.id, token: crypto.randomUUID(), kind: action, amount: cents, reason: action === "income" || action === "withdraw" ? normalized : "", ...(action !== "open" ? { sessionId: formTarget.current } : {}) };
      setFormError("");
      if (action === "close") setConfirmation(intent); else void submit(intent);
    } catch (value) { setFormError(value instanceof Error ? value.message : "Revisa los datos."); }
  }
  const locked = busy || Boolean(pending) || Boolean(storageError);
  const closeDetail = () => { if (!busy) { ++detailRequest.current; setDetailOpen(false); } };
  return <main id="main-content" tabIndex={-1} className="p-4 outline-none sm:p-6 min-[900px]:p-8">
    <h2 className="font-heading text-xl font-bold">Caja global</h2><p className="mt-2 text-sm text-muted">Una sola caja abierta para todos los operadores. Cerrar sesion no cierra la caja.</p>
    {storageError && <div role="alert" className="mt-4">{storageError}<Button disabled={busy} onClick={readStored}>Revisar almacenamiento</Button></div>}
    {pending && <section role="status" className="my-4 rounded-card border border-line p-4"><strong>Operacion pendiente: {labels[pending.kind]}</strong><p>{formatMoney(pending.amount)} · {pending.reason} {pending.sessionId ? `· Caja #${pending.sessionId}` : ""}</p><p>No cambies los datos: reintentar recupera la misma operacion, incluso despues del cierre.</p><Button disabled={busy} loading={busy} onClick={() => void submit(pending)}>Reintentar operacion</Button></section>}
    {formError && <p role="alert" className="my-3 text-danger">{formError}</p>}
    {error && <div role="alert" className="my-3 text-danger">{error}<Button disabled={busy || loading} onClick={() => void refresh()}>Reintentar consulta</Button></div>}
    <section className="my-5 rounded-card border border-line bg-surface p-4" aria-busy={loading}>
      {loading ? <span role="status"><Spinner />Consultando caja...</span> : current ? <Summary session={current} /> : !error && <EmptyState title="No hay caja abierta" description="Captura el fondo inicial para habilitar nuevos cobros." />}
      <div className="mt-4 flex flex-wrap gap-3">{!loading && !error && (current ? <>{(["income", "withdraw", "close"] as const).map((kind) => <Button key={kind} disabled={locked} onClick={() => start(kind)}>{labels[kind]}</Button>)}</> : <Button disabled={locked} onClick={() => start("open")}>Abrir caja</Button>)}</div>
    </section>
    {action && <form onSubmit={prepare} className="my-5 grid max-w-lg gap-4 rounded-card border border-line bg-surface p-4"><h3 className="font-bold">{labels[action]}</h3>{formSessionId !== undefined && <p>Caja destino #{formSessionId}</p>}<Field label={action === "open" ? "Fondo inicial (pesos)" : action === "close" ? "Efectivo contado (pesos)" : "Importe (pesos)"} inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} disabled={locked || Boolean(confirmation)} required />{(action === "income" || action === "withdraw") && <Field label="Motivo" value={reason} onChange={(e) => setReason(e.target.value)} disabled={locked} required />}<Button type="submit" disabled={locked || loading || Boolean(error) || Boolean(confirmation)}>{action === "close" ? "Revisar cierre final" : labels[action]}</Button><Button disabled={locked} onClick={() => { setAction(undefined); setConfirmation(undefined); formTarget.current = undefined; setFormSessionId(undefined); }}>Cancelar</Button></form>}
    <section className="rounded-card border border-line bg-surface p-4"><h3 className="font-bold">Historial de cajas</h3>{!sessions.length && !loading && <p className="mt-3">No hay sesiones registradas.</p>}<div className="mt-4 grid gap-3 sm:grid-cols-2">{sessions.map((s) => <article key={s.id} className="rounded-control border border-line p-3"><strong>Caja #{s.id} · {s.historical ? "Historica · Cerrada" : s.status === "open" ? "Abierta" : "Cerrada"}</strong><p className="text-sm">{formatDateTime(s.opened_at)} — {s.closed_at ? formatDateTime(s.closed_at) : "En curso"}</p><Button onClick={() => void showDetail(s.id)} disabled={busy}>Ver caja #{s.id}</Button></article>)}</div></section>
    <Modal open={Boolean(confirmation)} title="Confirmar cierre final" description="El cierre es definitivo: no se puede editar ni reabrir esta sesion. El backend valida el saldo final incluyendo ventas concurrentes." closeDisabled={busy} onClose={() => { if (!busy) setConfirmation(undefined); }} footer={<><Button disabled={busy} onClick={() => setConfirmation(undefined)}>Volver</Button><Button disabled={busy || loading || Boolean(error) || current?.id !== confirmation?.sessionId} loading={busy} onClick={() => { if (confirmation) void submit(confirmation); }}>Confirmar cierre</Button></>}>
      {confirmation && <><p>Caja destino #{confirmation.sessionId}</p><dl><dt>Esperado actual</dt><dd>{money(current?.expected_amount ?? null)}</dd><dt>Contado</dt><dd>{formatMoney(confirmation.amount)}</dd><dt>Diferencia prevista</dt><dd>{current?.expected_amount === null || current?.expected_amount === undefined ? "No registrado" : formatMoney(confirmation.amount - current.expected_amount)}</dd></dl></>}
    </Modal>
    <Modal open={detailOpen} title="Detalle de caja" onClose={closeDetail} closeDisabled={busy} footer={<Button disabled={busy} onClick={closeDetail}>Cerrar detalle</Button>} size="lg">
      {detailLoading ? <p role="status">Cargando detalle...</p> : detailError ? <div role="alert">{detailError}<Button onClick={() => selectedId && void showDetail(selectedId)}>Reintentar detalle</Button></div> : detail && <><Summary session={detail.session} /><h4 className="mt-5 font-bold">Movimientos auditados</h4>{!detail.operations.length && <p>No hay movimientos registrados.</p>}<ol className="mt-3 grid gap-3">{detail.operations.map((op) => <li key={op.id} className="break-words rounded-control border border-line p-3"><strong>{labels[op.kind]} · {formatMoney(op.amount)}</strong><p>{op.reason}</p><p className="text-sm">{op.operator} · {formatDateTime(op.created_at)}</p></li>)}</ol></>}
    </Modal>
  </main>;
}
