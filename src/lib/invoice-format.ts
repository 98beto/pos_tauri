/** Display the source decimal without converting it to a JavaScript number. */
export function formatInvoiceDecimal(
  value: string | null | undefined,
  currency?: string,
): string {
  if (value == null) return "No informado";
  return currency ? `${value} ${currency}` : value;
}

/** Invoice timestamps are local calendar values, not instants to convert. */
export function formatInvoiceDate(value: string | null | undefined): string {
  if (value == null) return "No informado";

  const match = /^(\d{4})-(\d{2})-(\d{2})(?:T(\d{2}:\d{2}:\d{2}))?$/.exec(value);
  if (!match) return value;

  const [, year, month, day, time] = match;
  return `${day}/${month}/${year}${time ? ` ${time}` : ""}`;
}

export function formatInvoiceNumber({
  series,
  folio,
  uuid,
}: {
  series?: string | null;
  folio?: string | null;
  uuid: string;
}): string {
  return [series, folio].filter((part): part is string => Boolean(part)).join("-") || uuid;
}
