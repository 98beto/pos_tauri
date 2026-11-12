import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { invoiceConcept, invoiceDocument } from "../../test/invoicefixtures";
import { InvoiceConceptsTable } from "./InvoiceConceptsTable";
import { InvoiceDocumentSummary } from "./InvoiceDocumentSummary";

describe("original invoice concepts", () => {
  it("preserves precision, absence vs zero, full descriptions and exempt taxes", () => {
    render(<InvoiceConceptsTable concepts={[invoiceConcept, { ...invoiceConcept, supplier_code: "Código propio", discount: "0", unit: null }]} currency="USD" />);
    const rows = screen.getAllByRole("row");
    expect(within(rows[1]).getByText("999999999999999999.123456 USD")).toBeTruthy();
    expect(within(rows[1]).getByText("0010.123456 USD")).toBeTruthy();
    expect(within(rows[1]).getByText("2.000")).toBeTruthy();
    expect(within(rows[2]).getByText("0 USD")).toBeTruthy();
    expect(within(rows[2]).getByText("No informado")).toBeTruthy();
    fireEvent.click(screen.getByText("Línea 1: atributos e impuestos"));
    expect(screen.getAllByText(/Factor: Exento/)).toHaveLength(2);
    expect(screen.getByRole("region", { name: "Tabla de conceptos desplazable" }).tabIndex).toBe(0);
  });
  it("paginates 101 lines in original order with document-wide optional columns", () => {
    render(<InvoiceConceptsTable concepts={Array.from({ length: 101 }, (_, index) => ({ ...invoiceConcept, description: `Original ${index}`, supplier_code: index === 100 ? "ÚLTIMO" : null, discount: index === 100 ? "0.000000" : null }))} currency="MXN" />);
    expect(screen.getAllByRole("row")).toHaveLength(51);
    expect(screen.getByRole("columnheader", { name: "Código del proveedor" })).toBeTruthy();
    expect(screen.getByRole("columnheader", { name: "Descuento" })).toBeTruthy();
    expect(screen.queryByText("Original 50")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Conceptos siguientes" }));
    expect(screen.getByText("Original 50")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Conceptos siguientes" }));
    expect(screen.getAllByRole("row")).toHaveLength(2);
    expect(screen.getByText("Original 100")).toBeTruthy();
    expect(screen.getByText("0.000000 MXN")).toBeTruthy();
  });
  it("does not introduce optional columns when all values are absent; displays original header values", () => {
    render(<InvoiceDocumentSummary document={invoiceDocument} />);
    expect(screen.queryByRole("columnheader", { name: "Descuento" })).toBeNull();
    expect(screen.queryByRole("columnheader", { name: "Código del proveedor" })).toBeNull();
    expect(screen.getByText("31/01/2026 23:59:59")).toBeTruthy();
    expect(screen.getByText("0 USD")).toBeTruthy();
    expect(screen.getByText(invoiceDocument.uuid)).toBeTruthy();
  });
  it("bounds the DOM to 50 concepts even for a 10,000-line document", () => {
    render(<InvoiceConceptsTable concepts={Array.from({ length: 10000 }, (_, index) => ({ ...invoiceConcept, description: `Documento extenso ${index}`, discount: index === 9999 ? "0" : null }))} currency="USD" />);
    expect(screen.getAllByRole("row")).toHaveLength(51);
    expect(screen.getByText("Conceptos: página 1 de 200 · 10000 líneas")).toBeTruthy();
    expect(screen.getByRole("columnheader", { name: "Descuento" })).toBeTruthy();
    expect(screen.queryByText("Documento extenso 9999")).toBeNull();
  });
});
