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
