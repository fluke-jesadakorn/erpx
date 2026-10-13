import {billingMonth,monthBounds} from "@/lib/billing/billing";
import {BusinessRuleError} from "../../shared/domain/error";
export type UsageMetrics = {
  ai: { requests: number; inputTokens: number; outputTokens: number };
  storage: { bytes: number; fileCount: number; averageDailyBytes: number };
};
export type UsageItem = {
  id: string;
  holdingId: string;
  companyId: string;
  companyName: string;
  branchId: string | null;
  branchName: string | null;
  type: "ai" | "storage";
  quantity: number;
  unit: "tokens" | "bytes";
  period: string;
  source: string;
  pricingStatus: "byok" | "simulation" | "unpriced";
  estimatedSatang: null;
  fundingSource?: "erpx" | "byok" | "simulation" | "unknown";
  inputTokens?: number;
  outputTokens?: number;
  occurredAt?: number;
  averageDailyBytes?: number;
  activeAtPeriodEnd?: boolean;
  requestStatus?: "succeeded" | "failed";
  gatewayStatus?: "reserved" | "succeeded" | "failed" | null;
  provider?: string | null;
  model?: string | null;
  unitsJson?: string | null;
  costStatus?: "unavailable" | "unverified" | "verified" | "reconciled" | "not_applicable";
  verifiedCostSatang?: number | null;
  costEvidenceKey?: string | null;
  costReconciledAt?: number | null;
  activeFrom?: number;
  activeUntil?: number;
  byteMilliseconds?: string;
};
export function platformUsageItems(items: UsageItem[]): UsageItem[] {
  const groups = new Map<string, UsageItem>();
  for (const item of items) {
    const key = [item.holdingId, item.companyId, item.branchId ?? "", item.type, item.pricingStatus].join(":");
    const existing = groups.get(key);
    if (existing) {
      existing.quantity += item.quantity;
      if (item.type === "ai") {
        existing.inputTokens = (existing.inputTokens ?? 0) + (item.inputTokens ?? 0);
        existing.outputTokens = (existing.outputTokens ?? 0) + (item.outputTokens ?? 0);
      }
    } else groups.set(key, {
      ...item, id: key, source: item.type === "ai" ? "AI provider" : "Original files",
      occurredAt: undefined,
    });
  }
  return [...groups.values()];
}
export function resolveUsageMonth(value:string|null,now:number):{month:string;start:number;end:number}{
 const month=value??billingMonth(now);
 if(!/^\d{4}-(0[1-9]|1[0-2])$/.test(month)||month>billingMonth(now))throw new BusinessRuleError(422,"INVALID_MONTH","Choose this month or an earlier month in YYYY-MM format.");
 const bounds=monthBounds(month);return {month,start:bounds.start,end:Math.min(bounds.end,now+1)};
}
