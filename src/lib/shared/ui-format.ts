/** UI chrome uses English names and Gregorian years independently of source-document language. */
export const uiLocale = "en-GB-u-ca-gregory";

export type TimePreference = {
  mode: "system" | "manual";
  timeZone: string | null;
  hourCycle: "h12" | "h23";
  version: number;
};

export const defaultTimePreference: TimePreference = {
  mode: "system", timeZone: null, hourCycle: "h23", version: 0,
};

export function validTimeZone(value: string): boolean {
  try { new Intl.DateTimeFormat("en", { timeZone: value }); return true; }
  catch { return false; }
}

export function systemTimeZone(): string {
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  return zone && validTimeZone(zone) ? zone : "UTC";
}

export function resolvedTimeZone(preference: TimePreference, systemZone = systemTimeZone()): string {
  return preference.mode === "manual" && preference.timeZone && validTimeZone(preference.timeZone)
    ? preference.timeZone : systemZone;
}

export function utcOffset(value: Date | number | string, timeZone: string): string {
  const instant = value instanceof Date ? value : new Date(value);
  if (!Number.isFinite(instant.getTime())) throw new RangeError("Invalid time value");
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone, timeZoneName: "shortOffset", year: "numeric", month: "2-digit", day: "2-digit",
  }).formatToParts(instant);
  const raw = parts.find(part => part.type === "timeZoneName")?.value ?? "GMT";
  const match = raw.match(/^(?:GMT|UTC)([+-])(\d{1,2})(?::(\d{2}))?$/);
  if (!match) return "UTC+00:00";
  return `UTC${match[1]}${match[2].padStart(2, "0")}:${match[3] ?? "00"}`;
}

export function formatPreferredDateTime(
  value: Date | number | string,
  preference: TimePreference,
  systemZone = systemTimeZone(),
  options: Intl.DateTimeFormatOptions = { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" },
) {
  const timeZone = resolvedTimeZone(preference, systemZone);
  const date = value instanceof Date ? value : new Date(value);
  const formatted = new Intl.DateTimeFormat(uiLocale, { ...options, timeZone, hourCycle: preference.hourCycle }).format(date);
  return options.timeStyle || options.hour || options.minute || options.second
    ? `${formatted} ${utcOffset(date, timeZone)}` : formatted;
}

export function formatUiDateTime(
  value: Date | number | string,
  timeZone = "Asia/Bangkok",
  options: Intl.DateTimeFormatOptions = { dateStyle: "medium", timeStyle: "short" },
) {
  const date = value instanceof Date ? value : new Date(value);
  const formatted = new Intl.DateTimeFormat(uiLocale, { ...options, timeZone }).format(date);
  return options.timeStyle || options.hour || options.minute || options.second
    ? `${formatted} ${utcOffset(date, timeZone)}` : formatted;
}

export function formatUiNumber(value: number, options?: Intl.NumberFormatOptions) {
  return new Intl.NumberFormat(uiLocale, options).format(value);
}
