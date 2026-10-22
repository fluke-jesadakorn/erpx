import type {UsageItem,UsageMetrics} from '@erpx/server-commercial/domain/usage';
import type {MonthlySeatSummary,ActiveSeatSnapshot} from '@erpx/server-commercial/domain/seats';
export type UsageBranch = { branchId: string; branchName: string; totals: UsageMetrics };
export type UsageCompany = {
  companyId: string;
  companyName: string;
  branches: UsageBranch[];
  unassigned: UsageMetrics;
  totals: UsageMetrics;
};
export type UsageHolding = {
  holdingId: string;
  holdingSlug: string;
  holdingName: string;
  companies: UsageCompany[];
  totals: UsageMetrics;
  sandboxStoragePreview: { status: "estimated" | "unpriced"; amountSatang: number | null };
  meterCoverage: { ai: "partial" | "measured"; storage: "partial" | "measured";
    seat: "unavailable" | "measured"; unreconciledOriginals: number };
  seatUsage: { monthly: MonthlySeatSummary; activeNow: ActiveSeatSnapshot };
};
export type UsageReport = {
  month: string;
  scope: "holding" | "branch" | "platform";
  holdings: UsageHolding[];
  items: UsageItem[];
  measured: true;
  coverage: "partial";
};

export type AiRow = {
  id: string; company_id: string; branch_id: string | null;
  model: string; feature: string; status: string | null; input_tokens: number;
  output_tokens: number; funding_source: "erpx" | "byok" | "simulation" | "unknown";
  created_at: number;
  gateway_status: "reserved" | "succeeded" | "failed" | null;
  provider: string | null; gateway_model: string | null; units_json: string | null;
  provider_statement_id: string | null; verified_cost_satang: number | null;
  verified_at: number | null; reconciled_at: number | null;
};
export type StorageRow = {
  id: string; company_id: string; branch_id: string | null;
  bytes: number; created_at: number; destroyed_at: number | null;
};
export type UsageNode = { id: string; kind: "company" | "branch"; display_name: string; company_id: string };


export function emptyUsageMetrics(): UsageMetrics {
  return { ai: { requests: 0, inputTokens: 0, outputTokens: 0 }, storage: { bytes: 0, fileCount: 0, averageDailyBytes: 0 } };
}
export function addUsageItem(metrics: UsageMetrics, item: UsageItem) {
  if (item.type === "ai") {
    metrics.ai.requests += 1;
    metrics.ai.inputTokens += item.inputTokens ?? 0;
    metrics.ai.outputTokens += item.outputTokens ?? 0;
  } else {
    if (item.activeAtPeriodEnd) {
      metrics.storage.bytes += item.quantity;
      metrics.storage.fileCount += 1;
    }
    metrics.storage.averageDailyBytes += item.averageDailyBytes ?? 0;
  }
}
export type UsageBinding={id:string;slug:string;displayName:string};
