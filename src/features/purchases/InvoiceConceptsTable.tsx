import { useState } from "react";
import { Button } from "../../components/ui";
import { formatInvoiceDecimal as decimal } from "../../lib/invoice-format";
import type { InvoiceConcept, InvoiceTax } from "../../types/purchase_invoice";

export function InvoiceTaxes({ taxes, currency }: { taxes: InvoiceTax[]; currency: string }) {
  return taxes.length ? <ul className="grid gap-2">{taxes.map((tax, index) => <li key={index} className="rounded-control border border-line p-2 text-xs">
    {tax.kind === "transferred" ? "Traslado" : "Retención"} · Impuesto {tax.code} · Base: {decimal(tax.base, currency)} · Factor: {tax.factor_type ?? "No informado"} · Tasa/cuota: {decimal(tax.rate)} · Importe: {decimal(tax.amount, currency)}
  </li>)}</ul> : <p>Sin impuestos informados</p>;
}

export function InvoiceConceptsTable({ concepts, currency }: { concepts: InvoiceConcept[]; currency: string }) {
  const [page, setPage] = useState(1);
  const size = 50;
  const pages = Math.max(1, Math.ceil(concepts.length / size));
  const current = Math.min(page, pages);
  // Optional columns belong to the entire document, not just the visible block.
  const code = concepts.some((line) => line.supplier_code !== null);
  const discount = concepts.some((line) => line.discount !== null);
  const cell = "border-b border-line p-3 align-top whitespace-normal break-words";
  return <section className="mt-5 grid gap-3" aria-label="Conceptos originales">
    <div role="region" aria-label="Tabla de conceptos desplazable" tabIndex={0} className="overflow-x-auto focus-visible:outline-2 focus-visible:outline-brand">
      <table className="w-full min-w-[800px] text-left text-sm">
        <caption className="sr-only">Conceptos originales del proveedor</caption>
        <thead><tr>{[...(code ? ["Código del proveedor"] : []), "Descripción", "Cantidad", "Unidad", "Precio unitario", ...(discount ? ["Descuento"] : []), "Importe", "Atributos e impuestos"].map((title) => <th key={title} scope="col" className={cell}>{title}</th>)}</tr></thead>
        <tbody>{concepts.slice((current - 1) * size, current * size).map((line, index) => <tr key={(current - 1) * size + index}>
          {code && <td className={cell}>{line.supplier_code ?? "No informado"}</td>}
          <td className={`${cell} min-w-64`}>{line.description}</td><td className={cell}>{decimal(line.quantity)}</td><td className={cell}>{line.unit ?? "No informado"}</td><td className={cell}>{decimal(line.unit_price, currency)}</td>
          {discount && <td className={cell}>{decimal(line.discount, currency)}</td>}<td className={cell}>{decimal(line.amount, currency)}</td>
          <td className={cell}><details><summary className="cursor-pointer">Línea {(current - 1) * size + index + 1}: atributos e impuestos</summary><p>ClaveProdServ: {line.product_service_code}</p><p>ClaveUnidad: {line.unit_code}</p><InvoiceTaxes taxes={line.taxes} currency={currency} /></details></td>
        </tr>)}</tbody>
      </table>
    </div>
    <nav aria-label="Paginación de conceptos" className="flex flex-wrap items-center gap-3"><Button disabled={current === 1} onClick={() => setPage(current - 1)}>Conceptos anteriores</Button><span role="status">Conceptos: página {current} de {pages} · {concepts.length} líneas</span><Button disabled={current === pages} onClick={() => setPage(current + 1)}>Conceptos siguientes</Button></nav>
  </section>;
}
