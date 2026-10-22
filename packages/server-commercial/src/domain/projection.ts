import type {UsageHolding,UsageBinding as Binding} from '@erpx/server-commercial/domain/holding-usage';
import type {UsageItem,UsageMetrics} from '@erpx/server-commercial/domain/usage';
import type {HoldingSeatMeter,SeatInterval} from '@erpx/server-commercial/domain/seats';
export type {Binding};
export type Snapshot = { holding: UsageHolding; items: UsageItem[]; seats?: HoldingSeatMeter };
export type Coverage = UsageHolding["meterCoverage"];
export type SnapshotHeader = { holding_id: string; ai_coverage: Coverage["ai"];
  storage_coverage: Coverage["storage"]; seat_coverage: Coverage["seat"];
  seat_source_count: number; unreconciled_originals: number;
  active_seat_count:number|null; active_seat_observed_at:number|null };
export type MeterRow = {
  holding_id: string; month: string; source_key: string; company_id: string; company_name: string;
  branch_id: string | null; branch_name: string | null; meter_kind: "ai" | "storage";
  funding_source: "erpx" | "byok" | "simulation" | "unknown";
  pricing_status: "byok" | "simulation" | "unpriced";
  quantity: number; input_tokens: number; output_tokens: number;
  average_daily_bytes: number; active_at_period_end: boolean;
  request_status: "succeeded" | "failed" | null;
  gateway_status: "reserved" | "succeeded" | "failed" | null;
  provider: string | null; model: string | null; units_json: string | null;
  cost_status: "unavailable" | "unverified" | "verified" | "reconciled" | "not_applicable";
  verified_cost_satang: number | null; cost_evidence_key: string | null;
  cost_reconciled_at: number | null;
  active_from: number | null; active_until: number | null; byte_milliseconds: string | null;
};


export function meterNumber(value: number | undefined, field: string): number {
  if (value === undefined) return 0;
  if (!Number.isSafeInteger(value) || value < 0) throw new Error(`INVALID_USAGE_METER:${field}`);
  return value;
}
export function emptyMetrics(): UsageMetrics {
  return { ai: { requests: 0, inputTokens: 0, outputTokens: 0 },
    storage: { bytes: 0, fileCount: 0, averageDailyBytes: 0 } };
}
export function addMeter(metrics: UsageMetrics, row: MeterRow) {
  if (row.meter_kind === "ai") {
    metrics.ai.requests += 1;
    metrics.ai.inputTokens += row.input_tokens;
    metrics.ai.outputTokens += row.output_tokens;
  } else {
    if (row.active_at_period_end) {
      metrics.storage.bytes += row.quantity;
      metrics.storage.fileCount += 1;
    }
    metrics.storage.averageDailyBytes += row.average_daily_bytes;
  }
}
export function shiftMonth(month: string, delta: number): string {
  const [year, number] = month.split("-").map(Number);
  return new Date(Date.UTC(year, number - 1 + delta, 1)).toISOString().slice(0, 7);
}
export type PrincipalAccount={user_id:string;status:string|null;platform_identity:boolean};
export type PrincipalEvent={id:number;user_id:string;status:string;effective_at:number};
export type ProjectedSeats={coverage:'measured'|'unavailable';rows:{userKey:string;intervals:SeatInterval[]}[]};
export type ProjectionPlan={binding:Binding;month:string;snapshotId:string;observedAt:number;coverage:Coverage;activeSeat:{count:number;asOf:number}|null};
