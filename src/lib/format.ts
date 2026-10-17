import type { BaseUnit } from "../types/product";
import type {
  PresentationMode,
  ProductPresentation,
} from "../types/product_presentation";

export const moneyFormatter = new Intl.NumberFormat("es-MX", {
  style: "currency",
  currency: "MXN",
});

export function formatMoney(cents: number) {
  return moneyFormatter.format(cents / 100);
}

export function centsToDecimal(cents: number) {
  return `${Math.trunc(cents / 100)}.${String(cents % 100).padStart(2, "0")}`;
}

const dateTimeFormatter = new Intl.DateTimeFormat("es-MX", {
  dateStyle: "medium",
  timeStyle: "short",
});

export function formatDateTime(value: string) {
  const isoValue = value.includes("T") ? value : `${value.replace(" ", "T")}Z`;
  const date = new Date(isoValue);
  return Number.isNaN(date.getTime()) ? value : dateTimeFormatter.format(date);
}

const quantityFormatter = new Intl.NumberFormat("es-MX", {
  maximumFractionDigits: 3,
});

export function formatBaseQuantity(quantity: number, baseUnit: BaseUnit) {
  if (baseUnit === "gram") {
    return `${quantityFormatter.format(quantity / 1_000)} kg`;
  }
  if (baseUnit === "milliliter") {
    return `${quantityFormatter.format(quantity / 1_000)} L`;
  }
  if (baseUnit === "centimeter") {
    return quantity < 100
      ? `${quantityFormatter.format(quantity)} cm`
      : `${quantityFormatter.format(quantity / 100)} m`;
  }
  return `${quantityFormatter.format(Math.trunc(quantity))} ${quantity === 1 ? "pieza" : "piezas"}`;
}

export function formatEnteredQuantity(
  quantity: number,
  mode: PresentationMode,
  presentationName: string,
  baseUnit: BaseUnit,
) {
  if (mode === "measured") return formatBaseQuantity(quantity, baseUnit);
  return `${quantityFormatter.format(Math.trunc(quantity))} ${presentationName}`;
}

export function formatPriceUnitLabel(baseUnit: BaseUnit, baseQuantity = 1_000) {
  if (baseUnit === "gram") {
    return baseQuantity === 1_000 ? "kg" : `${quantityFormatter.format(baseQuantity)} g`;
  }
  if (baseUnit === "milliliter") {
    return baseQuantity === 1_000 ? "litro" : `${quantityFormatter.format(baseQuantity)} ml`;
  }
  if (baseUnit === "centimeter") {
    return baseQuantity === 100 ? "metro" : `${quantityFormatter.format(baseQuantity)} cm`;
  }
  return "pieza";
}

export function formatPricePerUnit(price: number, baseUnit: BaseUnit, baseQuantity = 1_000) {
  return `${formatMoney(price)} por ${formatPriceUnitLabel(baseUnit, baseQuantity)}`;
}

export function presentationPriceLabel(
  presentation: Pick<ProductPresentation, "mode" | "name" | "price" | "base_quantity">,
  baseUnit: BaseUnit,
) {
  const unit = presentation.mode === "measured"
    ? formatPriceUnitLabel(baseUnit, presentation.base_quantity)
    : presentation.name;
  return `${formatMoney(presentation.price)} por ${unit}`;
}
