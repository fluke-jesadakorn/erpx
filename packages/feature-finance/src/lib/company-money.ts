import catalogue from "@erpx/catalogue/iso-4217-current.json";

export type CurrencyDefinition = { code: string; name: string; scale: number | null };
export type ManualRate = {
  id: string;
  currency: string;
  effectiveDate: string;
  currencyPerUsd: string;
  confirmedAt: number;
};
export type ResolvedRate = { rate: ManualRate; usedLatestFallback: boolean };

export const currencyDefinitions: CurrencyDefinition[] = catalogue.currencies;
const currencies = new Map(currencyDefinitions.map((item) => [item.code, item]));
const ratePrecision = 18;

export function currencyDefinition(code: string): CurrencyDefinition | undefined {
  return currencies.get(code.toUpperCase());
}

export function currencyScale(code: string, companyScale?: number | null): number {
  const definition = currencyDefinition(code);
  if (!definition) throw new Error(`Unknown ISO 4217 currency: ${code}`);
  const scale = definition.scale ?? companyScale;
  if (!Number.isInteger(scale) || scale === null || scale === undefined || scale < 0 || scale > 12)
    throw new Error(`${code} needs a company currency scale before transactions can be recorded.`);
  return scale;
}

export function decimalUnits(input: string, scale: number): bigint {
  if (!Number.isInteger(scale) || scale < 0 || scale > 18) throw new Error("Invalid decimal scale.");
  const match = input.trim().match(/^(-?)(\d+)(?:\.(\d+))?$/);
  if (!match) throw new Error("Enter a decimal amount.");
  const fraction = match[3] ?? "";
  if (fraction.length > scale && /[1-9]/.test(fraction.slice(scale))) throw new Error(`Amount supports ${scale} decimal places.`);
  const units = BigInt(match[2]) * BigInt(10) ** BigInt(scale) + BigInt(fraction.padEnd(scale, "0").slice(0, scale) || "0");
  return match[1] ? -units : units;
}

export function unitsDecimal(value: bigint | string, scale: number): string {
  const units = BigInt(value), negative = units < BigInt(0), absolute = negative ? -units : units;
  const base = BigInt(10) ** BigInt(scale);
  return `${negative ? "-" : ""}${absolute / base}${scale ? `.${String(absolute % base).padStart(scale, "0")}` : ""}`;
}

export function formatMoneyUnits(value: bigint | string, currency: string, scale: number, locale = "en-GB"): string {
  const units = BigInt(value), negative = units < BigInt(0), absolute = negative ? -units : units;
  const base = BigInt(10) ** BigInt(scale);
  const whole = new Intl.NumberFormat(locale).format(absolute / base);
  return `${negative ? "−" : ""}${currency} ${whole}${scale ? `.${String(absolute % base).padStart(scale, "0")}` : ""}`;
}

export function resolveRate(rates: ManualRate[], currency: string, date: string): ResolvedRate | null {
  if (currency === "USD") return { rate: { id: "USD", currency, effectiveDate: date, currencyPerUsd: "1", confirmedAt: 0 }, usedLatestFallback: false };
  const matches = rates.filter((rate) => rate.currency === currency).sort((a, b) =>
    b.effectiveDate.localeCompare(a.effectiveDate) || b.confirmedAt - a.confirmedAt || b.id.localeCompare(a.id));
  const dated = matches.find((rate) => rate.effectiveDate <= date);
  const latestConfirmed = matches.slice().sort((a, b) => b.confirmedAt - a.confirmedAt || b.id.localeCompare(a.id))[0];
  return dated ? { rate: dated, usedLatestFallback: false } : latestConfirmed ? { rate: latestConfirmed, usedLatestFallback: true } : null;
}

function divideRounded(numerator: bigint, denominator: bigint): bigint {
  if (denominator <= BigInt(0)) throw new Error("Invalid conversion denominator.");
  const negative = numerator < BigInt(0), absolute = negative ? -numerator : numerator;
  const rounded = (absolute + denominator / BigInt(2)) / denominator;
  return negative ? -rounded : rounded;
}

/** Both quotes are currency units per USD; rounding occurs once in target units. */
export function convertUnits(amountUnits: bigint | string, sourceScale: number, targetScale: number, sourcePerUsd: string, targetPerUsd: string): bigint {
  const source = decimalUnits(sourcePerUsd, ratePrecision), target = decimalUnits(targetPerUsd, ratePrecision);
  if (source <= BigInt(0) || target <= BigInt(0)) throw new Error("Manual rates must be positive.");
  const numerator = BigInt(amountUnits) * target * BigInt(10) ** BigInt(targetScale);
  const denominator = source * BigInt(10) ** BigInt(sourceScale);
  return divideRounded(numerator, denominator);
}

export function convertWithRates(input: { amountUnits: bigint | string; sourceCurrency: string; sourceScale: number; targetCurrency: string; targetScale: number; date: string; rates: ManualRate[] }) {
  const source = resolveRate(input.rates, input.sourceCurrency, input.date);
  const target = resolveRate(input.rates, input.targetCurrency, input.date);
  if (!source || !target) return null;
  return {
    amountUnits: convertUnits(input.amountUnits, input.sourceScale, input.targetScale, source.rate.currencyPerUsd, target.rate.currencyPerUsd),
    sourceRate: source.rate,
    targetRate: target.rate,
    usedLatestFallback: source.usedLatestFallback || target.usedLatestFallback,
  };
}
