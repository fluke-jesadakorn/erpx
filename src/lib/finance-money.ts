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
