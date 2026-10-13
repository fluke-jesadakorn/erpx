import { monthBounds } from "@/lib/billing/billing";

/**
 * Read-only Sandbox hypothesis. This catalog is intentionally separate from
 * billing_plans and billing_costs: it cannot authorize or issue an invoice.
 * Append a new effective-month version for a future experiment; retain old
 * versions so historical previews use the price that was shown at the time.
 */
export type SandboxPricingHypothesis = Readonly<{
  id: string;
  effectiveMonth: string;
  baseMonthlySatang: number;
  includedOriginalStorageBytes: number;
  storageSatangPerGbMonth: number;
  aiMarkupPercent: number;
  sellable: false;
}>;

export const SANDBOX_PRICING_HYPOTHESES: readonly SandboxPricingHypothesis[] = [
  {
    id: "sandbox-hypothesis-2026-09-v1",
    effectiveMonth: "2026-09",
    baseMonthlySatang: 149_000,
    includedOriginalStorageBytes: 20_000_000_000,
    storageSatangPerGbMonth: 500,
    aiMarkupPercent: 20,
    sellable: false,
  },
] as const;

export function sandboxPricingForMonth(
  month: string,
  versions: readonly SandboxPricingHypothesis[] = SANDBOX_PRICING_HYPOTHESES,
): SandboxPricingHypothesis | null {
  monthBounds(month);
  return [...versions]
    .filter((version) => version.effectiveMonth <= month)
    .sort((a, b) => b.effectiveMonth.localeCompare(a.effectiveMonth))[0] ?? null;
}

export type SandboxPricePreview =
  | { status: "estimated"; amountSatang: number }
  | { status: "byok"; amountSatang: 0 }
  | {
      status: "unpriced";
      amountSatang: null;
      reason: "funding_unknown" | "provider_cost_missing" | "storage_average_missing";
    };

const GB_BYTES = BigInt(1_000_000_000);

function safeSatang(value: bigint): number {
  if (value < BigInt(0) || value > BigInt(Number.MAX_SAFE_INTEGER))
    throw new RangeError("SANDBOX_PRICE_OUT_OF_RANGE");
  return Number(value);
}

function nonnegativeInteger(value: number, field: string): void {
  if (!Number.isSafeInteger(value) || value < 0)
    throw new RangeError(`INVALID_${field}`);
}

/** Requires a verified provider expense in satang; token counts are not prices. */
export function previewSandboxAiCost(
  input: {
    funding: "erpx" | "byok" | "unknown";
    providerCostSatang: number | null;
  },
  pricing: SandboxPricingHypothesis = SANDBOX_PRICING_HYPOTHESES[0],
): SandboxPricePreview {
  if (input.funding === "byok") return { status: "byok", amountSatang: 0 };
  if (input.funding === "unknown")
    return { status: "unpriced", amountSatang: null, reason: "funding_unknown" };
  if (input.providerCostSatang === null)
    return { status: "unpriced", amountSatang: null, reason: "provider_cost_missing" };
  nonnegativeInteger(input.providerCostSatang, "PROVIDER_COST");
  nonnegativeInteger(pricing.aiMarkupPercent, "AI_MARKUP");
  const markedUp =
    (BigInt(input.providerCostSatang) * BigInt(100 + pricing.aiMarkupPercent) + BigInt(50)) /
    BigInt(100);
  return { status: "estimated", amountSatang: safeSatang(markedUp) };
}

/**
 * Expects the monthly mean of daily original-file bytes across the Holding.
 * A point-in-time storage total is insufficient for a monthly price preview.
 * GB is decimal (1,000,000,000 bytes), rounded once to the nearest satang.
 */
export function previewSandboxStorage(
  input: { averageDailyOriginalBytes: number | null },
  pricing: SandboxPricingHypothesis = SANDBOX_PRICING_HYPOTHESES[0],
): SandboxPricePreview {
  if (input.averageDailyOriginalBytes === null)
    return { status: "unpriced", amountSatang: null, reason: "storage_average_missing" };
  nonnegativeInteger(input.averageDailyOriginalBytes, "STORAGE_AVERAGE");
  nonnegativeInteger(pricing.includedOriginalStorageBytes, "INCLUDED_STORAGE");
  nonnegativeInteger(pricing.storageSatangPerGbMonth, "STORAGE_PRICE");
  const excessBytes = Math.max(
    0,
    input.averageDailyOriginalBytes - pricing.includedOriginalStorageBytes,
  );
  const amount =
    (BigInt(excessBytes) * BigInt(pricing.storageSatangPerGbMonth) + GB_BYTES / BigInt(2)) /
    GB_BYTES;
  return { status: "estimated", amountSatang: safeSatang(amount) };
}
