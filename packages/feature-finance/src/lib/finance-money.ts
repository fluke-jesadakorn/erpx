/** Exact decimal helpers shared by finance domain code and deterministic tests. */
export function decimalToMinor(value: string, scale = 2): number {
  const match = value.trim().match(/^(-?)(\d+)(?:\.(\d+))?$/);
  if (!match) throw new Error("Money must be a decimal string.");
  const fraction = (match[3] ?? "").padEnd(scale, "0");
  if (fraction.length > scale && /[1-9]/.test(fraction.slice(scale))) throw new Error(`Money supports at most ${scale} decimal places.`);
  const minor = BigInt(match[2]) * BigInt(10) ** BigInt(scale) + BigInt(fraction.slice(0, scale) || "0");
  const signed = match[1] ? -minor : minor;
  if (signed > BigInt(Number.MAX_SAFE_INTEGER) || signed < BigInt(Number.MIN_SAFE_INTEGER)) throw new Error("Money amount is too large.");
  return Number(signed);
}

export function minorToDecimal(value: number, scale = 2): string {
  const negative = value < 0;
  const absolute = Math.abs(value);
  const base = 10 ** scale;
  const whole = Math.floor(absolute / base);
  const fraction = String(absolute % base).padStart(scale, "0");
  return `${negative ? "-" : ""}${whole}.${fraction}`;
}

/** Exact, unbounded decimal conversion for calculation engines and report snapshots. */
export function decimalToUnits(value: string, scale = 2): bigint {
  if (!Number.isInteger(scale) || scale < 0 || scale > 12) throw new Error("Currency scale must be between 0 and 12.");
  const match = value.trim().match(/^(-?)(\d+)(?:\.(\d+))?$/);
  if (!match) throw new Error("Money must be a decimal string.");
  const supplied = match[3] ?? "";
  if (supplied.length > scale && /[1-9]/.test(supplied.slice(scale))) throw new Error(`Money supports at most ${scale} decimal places.`);
  const fraction = supplied.padEnd(scale, "0").slice(0, scale);
  const units = BigInt(match[2]) * BigInt(10) ** BigInt(scale) + BigInt(fraction || "0");
  return match[1] ? -units : units;
}

export function unitsToDecimal(value: bigint | string, scale = 2): string {
  const units = typeof value === "bigint" ? value : BigInt(value);
  const negative = units < BigInt(0);
  const absolute = negative ? -units : units;
  if (scale === 0) return `${negative ? "-" : ""}${absolute}`;
  const base = BigInt(10) ** BigInt(scale);
  return `${negative ? "-" : ""}${absolute / base}.${String(absolute % base).padStart(scale, "0")}`;
}

/** Half-away-from-zero multiplication by basis points. */
export function multiplyBps(value: bigint, basisPoints: number): bigint {
  if (!Number.isInteger(basisPoints)) throw new Error("Basis points must be an integer.");
  const product = value * BigInt(basisPoints);
  const sign = product < BigInt(0) ? BigInt(-1) : BigInt(1);
  const absolute = product < BigInt(0) ? -product : product;
  return sign * ((absolute + BigInt(5_000)) / BigInt(10_000));
}

/** Largest-remainder allocation with stable key ordering; allocated units always equal total. */
export function allocateUnits(total: bigint, weights: Array<{ key: string; weight: bigint }>) {
  if (!weights.length || weights.some((item) => item.weight < BigInt(0))) throw new Error("Allocation requires nonnegative weights.");
  const denominator = weights.reduce((sum, item) => sum + item.weight, BigInt(0));
  if (denominator <= BigInt(0)) throw new Error("Allocation weights must total more than zero.");
  const sign = total < BigInt(0) ? BigInt(-1) : BigInt(1);
  const absolute = total < BigInt(0) ? -total : total;
  const rows = weights.map((item) => ({ key: item.key, units: (absolute * item.weight) / denominator, remainder: (absolute * item.weight) % denominator }));
  let remaining = absolute - rows.reduce((sum, item) => sum + item.units, BigInt(0));
  for (const row of [...rows].sort((a, b) => a.remainder === b.remainder ? a.key.localeCompare(b.key) : a.remainder > b.remainder ? -1 : 1)) {
    if (remaining === BigInt(0)) break;
    row.units += BigInt(1);
    remaining -= BigInt(1);
  }
  return rows.map(({ key, units }) => ({ key, units: units * sign }));
}
