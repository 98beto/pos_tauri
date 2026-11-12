import { formatInvoiceDate as date, formatInvoiceDecimal as decimal, formatInvoiceNumber } from "../../lib/invoice-format";
import type { InvoiceAttachmentMetadata, InvoiceDocument, PurchaseInvoiceAudit } from "../../types/purchase_invoice";
import { InvoiceConceptsTable, InvoiceTaxes } from "./InvoiceConceptsTable";

export function InvoiceAttachmentInfo({ attachment }: { attachment: InvoiceAttachmentMetadata }) {
  return <div className="my-2 break-all text-xs"><strong>{attachment.kind === "pdf" ? "PDF adjuntado por el usuario" : "XML original"}: {attachment.original_name}</strong><p>{attachment.mime_type} · {attachment.size_bytes} bytes</p><p>SHA-256: {attachment.sha256}</p></div>;
}

export function InvoiceDocumentSummary({ document: doc, audit }: { document: InvoiceDocument; audit?: PurchaseInvoiceAudit }) {
  const fields = [
    ["Proveedor / RFC", `${doc.issuer.name} · ${doc.issuer.rfc}`], ["Receptor / RFC", `${doc.receiver.name} · ${doc.receiver.rfc}`],
    ["Número de factura", formatInvoiceNumber(doc)], ["Fecha de emisión", date(doc.issued_at)], ["UUID", doc.uuid], ["Moneda", doc.currency],
    ["Versión / tipo", `${doc.version} / ${doc.document_type}`], ["Fecha de timbrado", date(doc.stamped_at)], ["Tipo de cambio", decimal(doc.exchange_rate)],
  ];
  return <>
    <dl className="grid gap-3 text-sm sm:grid-cols-2">{fields.map(([label, value]) => <div key={label} className="break-words"><dt className="text-xs text-muted">{label}</dt><dd>{value}</dd></div>)}</dl>
    {audit && <p className="mt-3 text-xs">Registro en la app: {audit.registered_at} (UTC) · {audit.registered_by_name} · Usuario #{audit.registered_by_user_id}</p>}
    <InvoiceConceptsTable concepts={doc.concepts} currency={doc.currency} />
    <section aria-label="Desglose original del comprobante" className="mt-5 grid gap-3 rounded-control border border-line p-4"><h3 className="font-bold">Desglose original del comprobante</h3><dl className="grid gap-2 text-sm">{[["Subtotal", doc.subtotal], ["Descuento", doc.discount], ["Impuestos trasladados", doc.total_transferred_taxes], ["Impuestos retenidos", doc.total_withheld_taxes], ["Total", doc.total]].map(([label, value]) => <div key={label} className="flex flex-wrap justify-between gap-3"><dt>{label}</dt><dd>{decimal(value, doc.currency)}</dd></div>)}</dl><InvoiceTaxes taxes={doc.taxes} currency={doc.currency} /></section>
  </>;
}
