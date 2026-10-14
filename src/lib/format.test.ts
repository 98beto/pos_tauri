import { describe, expect, it } from "vitest";
import {
  formatBaseQuantity,
  formatEnteredQuantity,
  formatPricePerUnit,
  formatPriceUnitLabel,
  presentationPriceLabel,
} from "./format";

describe("quantity formatting", () => {
  it("shows pieces as whole units", () => {
    expect(formatBaseQuantity(1, "piece")).toBe("1 pieza");
    expect(formatBaseQuantity(2_500, "piece")).toBe("2,500 piezas");
  });

  it("converts grams to kilograms and milliliters to liters with at most three decimals", () => {
    expect(formatBaseQuantity(250, "gram")).toBe("0.25 kg");
    expect(formatBaseQuantity(1_234, "gram")).toBe("1.234 kg");
    expect(formatBaseQuantity(1_500, "milliliter")).toBe("1.5 L");
  });

  it("formats entered quantities according to the presentation mode", () => {
    expect(formatEnteredQuantity(2, "fixed", "Bolsa 100", "piece")).toBe("2 Bolsa 100");
    expect(formatEnteredQuantity(375, "measured", "Kilogramo", "gram")).toBe("0.375 kg");
  });
});

describe("price labels", () => {
  it("labels base prices by piece, kilogram or liter", () => {
    expect(formatPriceUnitLabel("piece")).toBe("pieza");
    expect(formatPriceUnitLabel("gram")).toBe("kg");
    expect(formatPriceUnitLabel("milliliter")).toBe("litro");
    expect(formatPricePerUnit(12_000, "gram")).toContain("por kg");
  });

  it("uses the presentation name for fixed prices and the base unit for measured prices", () => {
    expect(presentationPriceLabel({ mode: "fixed", name: "Bolsa 100", price: 8_000, base_quantity: 100 }, "piece")).toContain("por Bolsa 100");
    expect(presentationPriceLabel({ mode: "measured", name: "Medio kilo", price: 6_000, base_quantity: 500 }, "gram")).toContain("por 500 g");
    expect(presentationPriceLabel({ mode: "measured", name: "Kilogramo", price: 12_000, base_quantity: 1_000 }, "gram")).toContain("por kg");
    expect(presentationPriceLabel({ mode: "measured", name: "Medio litro", price: 4_000, base_quantity: 500 }, "milliliter")).toContain("por 500 ml");
  });
});
