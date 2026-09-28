/** UI chrome uses English names and Gregorian years independently of source-document language. */
export const uiLocale = "en-GB-u-ca-gregory";

export function formatUiDateTime(
  value: Date | number | string,
  timeZone = "Asia/Bangkok",
  options: Intl.DateTimeFormatOptions = { dateStyle: "medium", timeStyle: "short" },
) {
  return new Intl.DateTimeFormat(uiLocale, { ...options, timeZone }).format(
    value instanceof Date ? value : new Date(value),
  );
}

export function formatUiNumber(value: number, options?: Intl.NumberFormatOptions) {
  return new Intl.NumberFormat(uiLocale, options).format(value);
}
