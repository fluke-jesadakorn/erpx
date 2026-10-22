import type { MetricData } from "@erpx/feature-shared/lib/executive/canvas";

export function exactUnits(value: string): bigint {
  if (!/^-?\d+$/.test(value)) throw new Error("Metric values must be exact integer units.");
  return BigInt(value);
}

/** Human-readable output uses the exact serialized value; only chart geometry is approximate. */
export function formatMetric(value: string, data: Pick<MetricData, "unit" | "currency" | "valueScale">): string {
  const units = exactUnits(value);
  if (data.unit === "count") return new Intl.NumberFormat("en-GB").format(units);
  if (data.unit === "duration") {
    const negative = units < BigInt(0), absolute = negative ? -units : units;
    const selected = absolute >= BigInt(86400000) ? { divisor: BigInt(86400000), suffix: "d" }
      : absolute >= BigInt(3600000) ? { divisor: BigInt(3600000), suffix: "h" }
        : absolute >= BigInt(60000) ? { divisor: BigInt(60000), suffix: "min" } : { divisor: BigInt(1000), suffix: "s" };
    const hundredths = (absolute * BigInt(100) + selected.divisor / BigInt(2)) / selected.divisor;
    const whole = hundredths / BigInt(100), fraction = String(hundredths % BigInt(100)).padStart(2, "0");
    const decimal = `${new Intl.NumberFormat("en-GB").format(whole)}.${fraction}`;
    return `${negative ? "-" : ""}${decimal} ${selected.suffix}`;
  }

  const negative = units < BigInt(0);
  const absolute = negative ? -units : units;
  const base = BigInt(10) ** BigInt(data.valueScale);
  const whole = absolute / base;
  const fraction = String(absolute % base).padStart(data.valueScale, "0");
  const formatter = new Intl.NumberFormat("en-GB", {
    style: data.unit === "money" ? "currency" : "decimal",
    ...(data.unit === "money" ? { currency: data.currency ?? "THB" } : {}),
    minimumFractionDigits: data.valueScale,
    maximumFractionDigits: data.valueScale,
  });
  const parts = formatter.formatToParts(negative ? (whole === BigInt(0) ? BigInt(-1) : -whole) : whole);
  const integerParts = new Intl.NumberFormat("en-GB", { useGrouping: true }).formatToParts(whole).filter((part) => part.type === "integer");
  let integerIndex = 0;
  const localizedDigits = [...fraction].map((digit) => new Intl.NumberFormat("en-GB", { useGrouping: false }).format(Number(digit))).join("");
  const formatted = parts.map((part) => part.type === "integer" ? integerParts[integerIndex++]?.value ?? "0"
    : part.type === "fraction" ? localizedDigits : part.type === "decimal" && !data.valueScale ? "" : part.value).join("");
  return data.unit === "percent" ? `${formatted}%` : formatted;
}

/** Return a bounded approximation suitable only for drawing a relative chart scale. */
export function chartUnits(value: string, maximum: bigint): number {
  if (maximum === BigInt(0)) return 0;
  const precision = BigInt(1_000_000);
  return Number(exactUnits(value) * precision / maximum) / Number(precision);
}

export function scaledChartUnits(maximum: bigint, numerator: number, denominator = 1_000_000): string {
  const divisor = BigInt(denominator);
  const multiplier = BigInt(Math.max(-denominator, Math.min(denominator, Math.round(numerator * denominator))));
  return String(maximum * multiplier / divisor);
}

export function absoluteMaximum(values: string[]): bigint {
  return values.reduce((maximum, value) => {
    const units = exactUnits(value), absolute = units < BigInt(0) ? -units : units;
    return absolute > maximum ? absolute : maximum;
  }, BigInt(0));
}

export function ratio(value: string, total: string): number {
  const denominator = exactUnits(total);
  if (denominator === BigInt(0)) return 0;
  const precision = BigInt(1_000_000);
  return Number(exactUnits(value) * precision / denominator) / Number(precision);
}
