import { describe, expect, it } from "vitest";
import { formatInvoiceDate, formatInvoiceDecimal, formatInvoiceNumber } from "./invoice-format";

describe("invoice decimal formatting", () => {
  it("preserves digits beyond JavaScript's safe integer range and source scale", () => {
    expect(formatInvoiceDecimal("90071992547409931234567890.123450", "MXN"))
      .toBe("90071992547409931234567890.123450 MXN");
    expect(formatInvoiceDecimal("0.000001", "USD")).toBe("0.000001 USD");
    expect(formatInvoiceDecimal("123.000000")).toBe("123.000000");
  });

  it("distinguishes absent values from zero and does not assume a currency", () => {
    expect(formatInvoiceDecimal(null, "MXN")).toBe("No informado");
    expect(formatInvoiceDecimal(undefined)).toBe("No informado");
    expect(formatInvoiceDecimal("0")).toBe("0");
    expect(formatInvoiceDecimal("0.000000", "USD")).toBe("0.000000 USD");
    expect(formatInvoiceDecimal("-12.3400", "MXN")).toBe("-12.3400 MXN");
  });
});

describe("invoice date formatting", () => {
  it("keeps the invoice calendar day and time at midnight without timezone conversion", () => {
    expect(formatInvoiceDate("2024-01-01T00:00:00")).toBe("01/01/2024 00:00:00");
    expect(formatInvoiceDate("2023-12-31T23:59:59")).toBe("31/12/2023 23:59:59");
    expect(formatInvoiceDate("2024-02-29")).toBe("29/02/2024");
  });

  it("returns unrecognized input unchanged and labels absent dates", () => {
    expect(formatInvoiceDate("2024-01-01T00:00:00Z")).toBe("2024-01-01T00:00:00Z");
    expect(formatInvoiceDate("<not a date>")).toBe("<not a date>");
    expect(formatInvoiceDate(null)).toBe("No informado");
    expect(formatInvoiceDate(undefined)).toBe("No informado");
  });
});

describe("invoice number formatting", () => {
  it("uses the available series and folio or falls back to the UUID", () => {
    expect(formatInvoiceNumber({ series: "A", folio: "00123", uuid: "uuid" })).toBe("A-00123");
    expect(formatInvoiceNumber({ series: "A", uuid: "uuid" })).toBe("A");
    expect(formatInvoiceNumber({ series: null, folio: "00123", uuid: "uuid" })).toBe("00123");
    expect(formatInvoiceNumber({ series: "", folio: "", uuid: "uuid" })).toBe("uuid");
    expect(formatInvoiceNumber({ uuid: "uuid" })).toBe("uuid");
  });
});
