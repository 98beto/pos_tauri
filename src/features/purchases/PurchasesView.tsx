import { useEffect, useId, useRef, useState } from "react";
import * as api from "../../api/purchase-invoices";
import { Button, EmptyState, Field, Modal, Spinner, ViewToggle } from "../../components/ui";
import { useViewPreference } from "../../hooks/useViewPreference";
import { errorMessage, isAppError, isSessionRequired } from "../../lib/app-error";
import { formatInvoiceDate, formatInvoiceDecimal, formatInvoiceNumber } from "../../lib/invoice-format";
import type { InvoiceSupplier, PurchaseInvoiceDetail, PurchaseInvoicePreview, PurchaseInvoicesView as Collection, PurchaseInvoicesViewFilters } from "../../types/purchase_invoice";
import { InvoiceAttachmentInfo, InvoiceDocumentSummary } from "./InvoiceDocumentSummary";

type Props = { sessionId: number; onSessionRequired: () => void };
const control = "h-11 w-full rounded-control border border-line-strong bg-surface px-3 text-sm";
const discard = (token: string) => { void api.discardInvoicePreview(token).catch(() => { /* Best effort on obsolete lifetimes only. */ }); };

// A new session owns a new lifetime, including filters, requests and native pickers.
export function PurchasesView(props: Props) { return <PurchasesCollection key={props.sessionId} {...props} />; }

function PurchasesCollection({ onSessionRequired }: Props) {
  const supplierSelectId = useId();
  const [layout, setLayout] = useViewPreference("pos.view.purchases", "list");
  const [filters, setFilters] = useState<PurchaseInvoicesViewFilters>({ page: 1 });
  const [invalidDateInput, setInvalidDateInput] = useState({ from: false, to: false });
  const [collection, setCollection] = useState<Collection>();
  const [loading, setLoading] = useState(true);
  const [listError, setListError] = useState("");
  const [refresh, setRefresh] = useState(0);
  const [supplierQuery, setSupplierQuery] = useState("");
  const [suppliers, setSuppliers] = useState<InvoiceSupplier[]>([]);
  const [supplierLoading, setSupplierLoading] = useState(true);
  const [supplierError, setSupplierError] = useState("");
  const [supplierRefresh, setSupplierRefresh] = useState(0);
  const [preview, setPreview] = useState<PurchaseInvoicePreview>();
  const previewRef = useRef<PurchaseInvoicePreview | undefined>(undefined);
  const [stale, setStale] = useState(false);
  const [detail, setDetail] = useState<PurchaseInvoiceDetail>();
  const [detailId, setDetailId] = useState<number>();
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState("");
  const [operationError, setOperationError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const inFlight = useRef(false);
  const lifetime = useRef(0);
  const listRequest = useRef(0);
  const supplierRequest = useRef(0);
  const detailRequest = useRef(0);
  useEffect(() => {
    ++lifetime.current;
    return () => { ++lifetime.current; ++listRequest.current; ++supplierRequest.current; ++detailRequest.current; if (previewRef.current) discard(previewRef.current.review_token); };
  }, []);

  function report(value: unknown, setError: (message: string) => void) {
    if (isSessionRequired(value)) onSessionRequired();
    else setError(errorMessage(value, "No se pudo completar la operación. Intenta nuevamente."));
  }

  const invalidDates = Boolean(invalidDateInput.from || invalidDateInput.to || (filters.date_from && !validDate(filters.date_from)) || (filters.date_to && !validDate(filters.date_to)) || (filters.date_from && filters.date_to && filters.date_from > filters.date_to));
  useEffect(() => {
    const request = ++listRequest.current;
    const generation = lifetime.current;
    setLoading(!invalidDates); setListError(""); setCollection(undefined);
    if (invalidDates) return;
    const timer = window.setTimeout(async () => {
      try { const result = await api.getPurchaseInvoicesView(filters); if (generation === lifetime.current && request === listRequest.current) setCollection(result); }
      catch (value) { if (generation === lifetime.current && request === listRequest.current) report(value, setListError); }
      finally { if (generation === lifetime.current && request === listRequest.current) setLoading(false); }
    }, 250);
    return () => { window.clearTimeout(timer); ++listRequest.current; };
  }, [filters, refresh, invalidDates]);

  useEffect(() => {
    const request = ++supplierRequest.current;
    const generation = lifetime.current;
    setSupplierLoading(true); setSupplierError(""); setSuppliers([]);
    const timer = window.setTimeout(async () => {
      try { const result = await api.searchInvoiceSuppliers(supplierQuery.trim() || undefined); if (generation === lifetime.current && request === supplierRequest.current) setSuppliers(result); }
      catch (value) { if (generation === lifetime.current && request === supplierRequest.current) report(value, setSupplierError); }
      finally { if (generation === lifetime.current && request === supplierRequest.current) setSupplierLoading(false); }
    }, 250);
    return () => { window.clearTimeout(timer); ++supplierRequest.current; };
  }, [supplierQuery, supplierRefresh]);

  function updateFilters(next: Partial<PurchaseInvoicesViewFilters>) {
    ++listRequest.current; setCollection(undefined); setLoading(true); setListError("");
    setFilters((old) => ({ ...old, ...next, page: next.page ?? 1 }));
  }
  function updatePreview(next: PurchaseInvoicePreview | undefined) { previewRef.current = next; setPreview(next); }

  // Native mutations/pickers share a synchronous lock: React state alone cannot prevent double clicks.
  async function operation(action: (current: () => boolean) => Promise<void>) {
    if (inFlight.current) return;
    inFlight.current = true; setBusy(true); setOperationError("");
    const generation = lifetime.current;
    const current = () => lifetime.current === generation;
    try { await action(current); }
    catch (value) {
      if (current()) {
        if (isAppError(value) && value.code === "INVOICE_PREVIEW_STALE") setStale(true);
        report(value, setOperationError);
      }
    } finally { if (current()) { inFlight.current = false; setBusy(false); } }
  }

  function pickXml() {
    void operation(async (current) => {
      const result = await api.pickInvoiceXml();
      if (!current()) { if (result) discard(result.review_token); return; }
      if (result) { updatePreview(result); setStale(false); setNotice(""); }
    });
  }
  function closePreview() {
    if (inFlight.current || !preview) return;
    const token = preview.review_token;
    void operation(async (current) => {
      await api.discardInvoicePreview(token);
      if (current()) { updatePreview(undefined); setStale(false); }
    });
  }
  function register() {
    if (!preview || stale) return;
    const token = preview.review_token;
    void operation(async (current) => {
      const result = await api.registerInvoice(token);
      if (!current()) return;
      updatePreview(undefined);
      setDetail(result.detail); setDetailId(result.detail.id); setDetailError("");
      setNotice(result.outcome === "created" ? "Factura registrada. No se registró mercancía ni un pago." : "La factura ya estaba registrada; se recuperó el documento existente. El PDF de la revisión no se añade al duplicado.");
      setRefresh((value) => value + 1);
      setSupplierRefresh((value) => value + 1);
      void api.discardInvoicePreview(token).catch((value: unknown) => {
        if (current() && isSessionRequired(value)) onSessionRequired();
      });
    });
  }
  async function loadDetail(id: number) {
    const request = ++detailRequest.current;
    const generation = lifetime.current;
    setDetailId(id); setDetail(undefined); setDetailLoading(true); setDetailError(""); setOperationError(""); setNotice("");
    try { const result = await api.getPurchaseInvoiceDetail(id); if (generation === lifetime.current && request === detailRequest.current) setDetail(result); }
    catch (value) { if (generation === lifetime.current && request === detailRequest.current) report(value, setDetailError); }
    finally { if (generation === lifetime.current && request === detailRequest.current) setDetailLoading(false); }
  }
  function closeDetail() { if (inFlight.current) return; ++detailRequest.current; setDetailId(undefined); setDetail(undefined); setDetailLoading(false); setOperationError(""); setNotice(""); }

  const hasFilters = Boolean(filters.supplier_rfc || filters.number || filters.date_from || filters.date_to);
  const selectedSupplier = suppliers.find((s) => s.rfc === filters.supplier_rfc);
  const rows = collection?.invoices ?? [];
  const detailAction = (id: number) => <Button size="sm" onClick={() => void loadDetail(id)}>Ver detalle</Button>;
  return <main id="main-content" tabIndex={-1} className="p-4 outline-none sm:p-6 min-[900px]:p-8">
    <section className="overflow-hidden rounded-card border border-line bg-surface">
      <header className="flex flex-wrap items-center justify-between gap-3 border-b border-line p-4 sm:p-5"><div><h2 className="font-heading text-xl font-bold">Facturas de compra</h2><p className="mt-1 text-sm text-muted">Administra y consulta facturas de proveedores.</p></div><div className="flex flex-wrap items-center gap-3"><ViewToggle label="compras" value={layout} onChange={setLayout} /><Button variant="primary" disabled={busy || Boolean(preview) || detailId !== undefined} onClick={pickXml}><svg aria-hidden="true" className="size-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth="2"><path strokeLinecap="round" strokeLinejoin="round" d="M12 16V4m0 0L8 8m4-4 4 4M4 16v3a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-3" /></svg>Importar factura</Button></div></header>
      <div className="border-b border-line bg-surface-soft/60 p-4 sm:p-5">
        <div className="grid items-start gap-4 lg:grid-cols-3">
        <div className="grid content-start gap-3">
        <Field label="Buscar proveedor por nombre o RFC" maxLength={100} value={supplierQuery} onChange={(event) => { ++supplierRequest.current; setSuppliers([]); setSupplierLoading(true); setSupplierQuery(event.target.value); }} />
        <div className="grid gap-2"><label htmlFor={supplierSelectId} className="text-[0.625rem] font-extrabold tracking-[0.08em] text-muted uppercase">Proveedor</label><select id={supplierSelectId} className={control} value={filters.supplier_rfc ?? ""} onChange={(event) => updateFilters({ supplier_rfc: event.target.value || undefined })}><option value="">Todos los proveedores</option>{filters.supplier_rfc && !selectedSupplier && <option value={filters.supplier_rfc}>{filters.supplier_rfc} (seleccionado)</option>}{suppliers.map((supplier) => <option key={supplier.rfc} value={supplier.rfc}>{supplier.name} · {supplier.rfc}</option>)}</select><span role="status" className="text-xs">{supplierLoading ? "Buscando proveedores…" : `${suppliers.length} proveedores encontrados`}</span></div>
        </div>
        <Field label="N.º de factura" hint="Serie, folio, serie-folio o UUID parcial" maxLength={100} value={filters.number ?? ""} onChange={(event) => updateFilters({ number: event.target.value || undefined })} />
        <div className="grid content-start gap-3 sm:grid-cols-2">
        <Field label="Fecha de emisión desde" type="date" value={filters.date_from ?? ""} onInput={(event) => { const invalid = event.currentTarget.validity.badInput; setInvalidDateInput((old) => ({ ...old, from: invalid })); }} onChange={(event) => updateFilters({ date_from: event.target.value || undefined })} />
        <Field label="Fecha de emisión hasta" type="date" value={filters.date_to ?? ""} onInput={(event) => { const invalid = event.currentTarget.validity.badInput; setInvalidDateInput((old) => ({ ...old, to: invalid })); }} onChange={(event) => updateFilters({ date_to: event.target.value || undefined })} />
        </div>
        </div>
        <div className="mt-4 flex justify-end"><Button onClick={() => { updateFilters({ supplier_rfc: undefined, number: undefined, date_from: undefined, date_to: undefined }); setInvalidDateInput({ from: false, to: false }); setSupplierQuery(""); }}>Limpiar filtros</Button></div>
        {supplierError && <div role="alert" className="mt-3 flex flex-wrap items-center gap-3 text-sm text-danger">Error al buscar proveedores: {supplierError}<Button size="sm" onClick={() => setSupplierRefresh((value) => value + 1)}>Reintentar proveedores</Button></div>}
        {invalidDates && <p role="alert" className="mt-3 text-sm text-danger">Indica fechas de emisión válidas; desde debe ser anterior o igual a hasta. Los extremos son inclusivos.</p>}
      </div>
      {listError && <div role="alert" className="flex flex-wrap items-center gap-3 p-4 text-sm text-danger sm:p-5">No se pudo actualizar el listado: {listError}<Button size="sm" onClick={() => setRefresh((value) => value + 1)}>Reintentar listado</Button></div>}
      {!preview && detailId === undefined && operationError && <p role="alert" className="p-4 text-sm text-danger sm:p-5">{operationError}</p>}
      {loading && <div role="status" className="flex min-h-64 items-center justify-center gap-3 text-sm font-semibold text-muted"><Spinner role="presentation" aria-hidden="true" className="size-6 text-brand" />Cargando facturas…</div>}
      {!loading && !invalidDates && !listError && collection && <>
        <p className="px-4 py-3 text-sm text-muted sm:px-5" role="status">{collection.total} facturas en resultados</p>
        {rows.length ? layout === "list" ? <div role="region" aria-label="Facturas de compra desplazables" tabIndex={0} className="overflow-x-auto focus-visible:outline-2 focus-visible:outline-brand"><table className="w-full min-w-[720px] text-left text-sm"><caption className="sr-only">Facturas de compra</caption><thead><tr className="text-xs font-bold tracking-wider text-muted uppercase">{["Fecha de emisión", "Proveedor / RFC", "Número de factura", "Total", "PDF", "Acciones"].map((title) => <th scope="col" key={title} className={`border-b border-line px-4 py-3 ${title === "Total" || title === "Acciones" ? "text-right" : ""}`}>{title}</th>)}</tr></thead><tbody>{rows.map((invoice) => <tr key={invoice.id} className="hover:bg-surface-soft/60"><td className="border-b border-line px-4 py-3">{formatInvoiceDate(invoice.issued_at)}</td><td className="border-b border-line px-4 py-3"><strong>{invoice.issuer.name}</strong><span className="block text-xs text-muted">{invoice.issuer.rfc}</span></td><td className="break-all border-b border-line px-4 py-3">{formatInvoiceNumber(invoice)}</td><td className="border-b border-line px-4 py-3 text-right">{formatInvoiceDecimal(invoice.total, invoice.currency)}</td><td className="border-b border-line px-4 py-3">{invoice.has_pdf ? "Adjuntado por el usuario" : "Sin PDF"}</td><td className="border-b border-line px-4 py-3 text-right">{detailAction(invoice.id)}</td></tr>)}</tbody></table></div> : <div className="grid gap-3 p-4 sm:grid-cols-2 sm:p-5 xl:grid-cols-3">{rows.map((invoice) => <article key={invoice.id} className="flex flex-col rounded-control border border-line p-4"><h3 className="break-all font-bold">{formatInvoiceNumber(invoice)}</h3><p className="mt-2 text-sm font-semibold">{invoice.issuer.name}</p><p className="text-xs text-muted">{invoice.issuer.rfc}</p><p className="mt-3 text-sm text-muted">{formatInvoiceDate(invoice.issued_at)}</p><p className="mt-2 text-sm font-bold">{formatInvoiceDecimal(invoice.total, invoice.currency)}</p><p className="mt-2 text-xs text-muted">{invoice.has_pdf ? "Adjuntado por el usuario" : "Sin PDF"}</p><div className="mt-auto flex justify-end pt-4">{detailAction(invoice.id)}</div></article>)}</div> : <EmptyState title={hasFilters ? "No hay facturas con estos filtros" : "Aún no hay facturas"} description="Importa un XML o ajusta los filtros de búsqueda." icon="#" />}
        <nav aria-label="Paginación de facturas" className="flex flex-wrap items-center justify-end gap-3 border-t border-line p-4 text-sm text-muted sm:px-5"><Button disabled={collection.page <= 1} onClick={() => updateFilters({ page: collection.page - 1 })}>Anterior</Button><span>Página {collection.page} · 25 por página</span><Button disabled={collection.page * collection.page_size >= collection.total} onClick={() => updateFilters({ page: collection.page + 1 })}>Siguiente</Button></nav>
      </>}
    </section>
    <Modal open={Boolean(preview)} title="Revisar factura" size="lg" onClose={closePreview} closeDisabled={busy} footer={<><Button disabled={busy} onClick={closePreview}>Cancelar revisión</Button><Button disabled={busy || stale} onClick={register}>Registrar factura</Button></>}>
      {preview && <><p className="mb-4 font-bold">Registrar el documento no registra mercancía recibida ni un pago.</p>{operationError && <p role="alert">{operationError}</p>}{stale ? <p role="alert">La revisión ya no está disponible. Cancela esta revisión y vuelve a seleccionar el XML para revisarlo antes de registrar.</p> : <Button disabled={busy} onClick={() => void operation(async (current) => { const result = await api.pickInvoicePreviewPdf(preview.review_token); if (!current()) { if (result) discard(result.review_token); return; } if (result) updatePreview(result); })}>Adjuntar PDF a revisión</Button>}<InvoiceAttachmentInfo attachment={preview.xml} />{preview.pdf ? <InvoiceAttachmentInfo attachment={preview.pdf} /> : <p>Sin PDF adjunto</p>}<InvoiceDocumentSummary key={preview.review_token} document={preview.document} /></>}
    </Modal>
    <Modal open={detailId !== undefined} title="Detalle de factura" size="lg" onClose={closeDetail} closeDisabled={busy} footer={<Button disabled={busy} onClick={closeDetail}>Cerrar</Button>}>
      {notice && <p role="status" className="mb-3 font-bold">{notice}</p>}{listError && detail && <p role="alert">El documento sigue registrado. Falló la actualización del listado: {listError}</p>}{detailLoading && <p role="status">Cargando detalle…</p>}{detailError && <div role="alert">{detailError}<Button onClick={() => detailId !== undefined && void loadDetail(detailId)}>Reintentar detalle</Button></div>}{operationError && <p role="alert">{operationError}</p>}
      {detail && <><div className="mb-4 flex flex-wrap gap-2"><Button disabled={busy} onClick={() => void operation(async () => { await api.exportInvoiceAttachment(detail.id, "xml"); })}>Exportar XML</Button>{detail.pdf ? <><Button disabled={busy} onClick={() => void operation(async () => { await api.exportInvoiceAttachment(detail.id, "pdf"); })}>Exportar PDF</Button><Button disabled={busy} onClick={() => void operation(async () => { await api.openInvoicePdf(detail.id); })}>Abrir PDF</Button></> : <Button disabled={busy} onClick={() => void operation(async (current) => { const result = await api.attachInvoicePdf(detail.id); if (current() && result) { setDetail(result); setRefresh((value) => value + 1); } })}>Adjuntar PDF</Button>}</div><InvoiceAttachmentInfo attachment={detail.xml} />{detail.pdf ? <InvoiceAttachmentInfo attachment={detail.pdf} /> : <p>Sin PDF adjunto</p>}<InvoiceDocumentSummary key={detail.id} document={detail.document} audit={detail.audit} /></>}
    </Modal>
  </main>;
}

function validDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || value.startsWith("0000")) return false;
  // Calendar validation only; never convert a document timestamp to an instant.
  const [year, month, day] = value.split("-").map((part) => parseInt(part, 10));
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  return month >= 1 && month <= 12 && day >= 1 && day <= days[month - 1];
}
