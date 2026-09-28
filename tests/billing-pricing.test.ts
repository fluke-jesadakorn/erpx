import { expect, it } from "vitest";
import {
  SANDBOX_PRICING_HYPOTHESES,
  previewSandboxAiCost,
  previewSandboxStorage,
  sandboxPricingForMonth,
  type SandboxPricingHypothesis,
} from "../src/lib/billing-pricing";

it("keeps the 1,490 baht proposal explicitly unsellable and separate from approved Support plans", () => {
  expect(sandboxPricingForMonth("2026-08")).toBeNull();
  expect(sandboxPricingForMonth("2026-09")).toMatchObject({
    id: "sandbox-hypothesis-2026-09-v1",
    baseMonthlySatang: 149_000,
    includedOriginalStorageBytes: 20_000_000_000,
    storageSatangPerGbMonth: 500,
    aiMarkupPercent: 20,
    sellable: false,
  });
  expect(() => sandboxPricingForMonth("2026-13")).toThrow("INVALID_BILLING_MONTH");
});

it("selects the effective version without rewriting an older month", () => {
  const next: SandboxPricingHypothesis = {
    ...SANDBOX_PRICING_HYPOTHESES[0],
    id: "sandbox-hypothesis-2026-11-v2",
    effectiveMonth: "2026-11",
    baseMonthlySatang: 169_000,
  };
  const versions = [...SANDBOX_PRICING_HYPOTHESES, next];
  expect(sandboxPricingForMonth("2026-10", versions)?.id).toBe(
    "sandbox-hypothesis-2026-09-v1",
  );
  expect(sandboxPricingForMonth("2026-11", versions)?.id).toBe(next.id);
});

it("prices provider-funded AI only when verified cost exists and excludes BYOK", () => {
  expect(previewSandboxAiCost({ funding: "erpx", providerCostSatang: null })).toEqual({
    status: "unpriced",
    amountSatang: null,
    reason: "provider_cost_missing",
  });
  expect(previewSandboxAiCost({ funding: "unknown", providerCostSatang: 7500 })).toEqual({
    status: "unpriced",
    amountSatang: null,
    reason: "funding_unknown",
  });
  expect(previewSandboxAiCost({ funding: "byok", providerCostSatang: null })).toEqual({
    status: "byok",
    amountSatang: 0,
  });
  expect(previewSandboxAiCost({ funding: "erpx", providerCostSatang: 7500 })).toEqual({
    status: "estimated",
    amountSatang: 9000,
  });
  expect(previewSandboxAiCost({ funding: "erpx", providerCostSatang: 1 })).toEqual({
    status: "estimated",
    amountSatang: 1,
  });
  expect(() => previewSandboxAiCost({ funding: "erpx", providerCostSatang: -1 })).toThrow();
});

it("prices only measured monthly average original storage above decimal 20 GB", () => {
  expect(previewSandboxStorage({ averageDailyOriginalBytes: null })).toEqual({
    status: "unpriced",
    amountSatang: null,
    reason: "storage_average_missing",
  });
  expect(previewSandboxStorage({ averageDailyOriginalBytes: 19_000_000_000 })).toEqual({
    status: "estimated",
    amountSatang: 0,
  });
  expect(previewSandboxStorage({ averageDailyOriginalBytes: 28_000_000_000 })).toEqual({
    status: "estimated",
    amountSatang: 4000,
  });
  expect(previewSandboxStorage({ averageDailyOriginalBytes: 20_010_000_000 })).toEqual({
    status: "estimated",
    amountSatang: 5,
  });
  expect(() => previewSandboxStorage({ averageDailyOriginalBytes: -1 })).toThrow();
});
