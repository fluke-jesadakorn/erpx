
import {type TargetPrice} from "@/lib/billing-target-pricing";
export type AssignmentRow = {
  holding_id: string; effective_month: string; price_id: string;
  mode: "shadow" | "sandbox_invoice"; service_started_at: string;
};
export type PriceRow = {
  id: string; effective_month: string; base_monthly_satang: string;
  included_original_bytes: string; storage_satang_per_gb_month: string;
  ai_markup_percent: number;
  seat_terms_status: "undefined" | "defined";
  included_seats: number | null;
  seat_monthly_satang: string | null;
  storage_allowance_mode: "holding_flat" | "per_paid_seat_time";
  rating_enabled: boolean;
};
export type SnapshotRow = {
  snapshot_id: string; refreshed_at: string; source_count: number; ai_coverage: "measured" | "partial";
  storage_coverage: "measured" | "partial"; unreconciled_originals: number;
  seat_coverage: "unavailable" | "measured"; seat_source_count: number;
};
export type EvidenceMeterRow = MeterRow & {
  active_at_period_end: boolean;
  byte_milliseconds: string | null;
  snapshot_id: string;
};
export type TargetMeasuredEvidence = {
  snapshot: {
    id: string; refreshedAt: number; sourceCount: number; seatSourceCount: number;
    aiCoverage: "measured" | "partial"; storageCoverage: "measured" | "partial";
    seatCoverage: "measured" | "unavailable";
  } | null;
  ai: {
    requests: number | null; inputTokens: number | null; outputTokens: number | null;
    platformSucceeded: number; platformVerified: number; platformPending: number;
    byok: number; failed: number; simulation: number; unknown: number;
    verifiedProviderCostSatang: number | null;
    chargeableVerifiedProviderCostSatang: number | null;
    totalProviderCostSatang: number | null;
    sources: Array<{
      sourceKey: string; companyId: string; branchId: string | null;
      provider: string | null; fundingSource: MeterRow["funding_source"];
      requestStatus: string | null; gatewayStatus: string | null;
      providerUnits: Record<string, number> | null; costStatus: string | null;
      verifiedProviderCostSatang: number | null; costEvidenceKey: string | null;
      billableStatus: "verified_cost" | "byok" | "excluded_failed" | "simulation" | "price_unavailable";
    }>;
  };
  storage: {
    originalCount: number | null; activeBytesAtEnd: number | null;
    byteMilliseconds: string | null; coverage: "measured" | "partial" | "unavailable";
  };
  seats: {
    distinctUsers: number | null; activeSeatMilliseconds: string | null;
    coverage: "measured" | "unavailable";
    activeNow: { status: "measured" | "unavailable"; count: number | null; asOf: number | null };
  };
  operatingCosts: {
    categories: Record<"infrastructure" | "backup" | "stripe" | "support",
      { status: "verified" | "unavailable"; amountSatang: number | null }>;
    totalVerifiedSatang: number | null;
  };
};
export type MeterRow = {
  source_key: string; meter_kind: "ai" | "storage";
  company_id: string; branch_id: string | null;
  funding_source: "erpx" | "byok" | "simulation" | "unknown";
  quantity: string; input_tokens: string; output_tokens: string;
  request_status: string | null; provider: string | null; model: string | null;
  gateway_status: string | null;
  units_json: Record<string, number> | string | null; cost_status: string | null;
  verified_cost_satang: string | null; cost_evidence_key: string | null;
  cost_reconciled_at: string | null;
  active_from: string | null; active_until: string | null;
};
export type CostRow = {
  source_key: string; provider: string; statement_id: string;
  original_currency: string; original_minor: string;
  provider_cost_satang: string; evidence_id: string;
};
export type StatementRow = {
  provider: string; statement_id: string; original_currency: string;
  provider_total_minor: string; reconciled_total_minor: string;
  evidence_id: string; verified_at: string;
};
export type TargetRatedLine = {
  kind: "base" | "storage" | "ai" | "extra" | "seat";
  description: string;
  amountSatang: number | null;
  sourceKey: string | null;
  measuredQuantity: string | null;
  verifiedProviderCostSatang: number | null;
  evidenceId: string | null;
  statementId: string | null;
  reconciliationStatus: "verified" | "missing" | "not_required";
  provider: string | null;
  fundingSource: "erpx" | "byok" | "simulation" | "unknown" | null;
  requestStatus: string | null;
  providerUnits: Record<string, number> | null;
};
export type TargetBillingPeriodView = {
  holdingId: string;
  month: string;
  assignment: { effectiveMonth: string; mode: "shadow" | "sandbox_invoice"; priceId: string; serviceStartedAt: number } | null;
  price: TargetPrice | null;
  measuredEvidence: TargetMeasuredEvidence;
  completeness: { ready: boolean; missing: string[]; meterSnapshotId: string | null };
  approval: { eligible: boolean; blockers: string[] };
  lines: TargetRatedLine[];
  sources: TargetRatedLine[];
  statementReconciliation: { provider: string; statementId: string; originalCurrency: string; providerTotalMinor: string; reconciledTotalMinor: string; status: "verified" | "mismatch" }[];
  totalSatang: number | null;
  invoice: { id: string; totalSatang: number; status: string; stripeId: string | null } | null;
  state: "unassigned" | "held" | "ready" | "approved";
};
export type OperatingCostsRow={
    category: "infrastructure" | "backup" | "stripe" | "support";
    amount_satang: string; source_state: string;
  };

export type MeasuredSnapshotRow=SnapshotRow;

export type CurrentSeatsRow={
    active_seat_count: number | null; active_seat_observed_at: string | null;
  };

export type MeasuredMetersRow=EvidenceMeterRow;

export type MeasuredSeatIntervalsRow={ intervals_json: unknown; snapshot_id: string };

export type AssignedOfferingRow=AssignmentRow;

export type AssignedPriceRow=PriceRow;

export type ApprovedRatingRow={
    state: string; missing_json: string; lines_json: string; total_satang: string;
    meter_snapshot_id: string; invoice_id: string;
  };

export type ApprovedInvoiceRow={id:string;total_satang:string;status:string;stripe_id:string|null};

export type ApprovedStatementsRow=StatementRow;

export type RolloutEligibilityRow=Record<string,unknown>;

export type LegacyEnrollmentOverlapRow=Record<string,unknown>;

export type MonthInvoicesRow=Record<string,unknown>;

export type PaymentConsentRow={
    payment_mode: string; consent_at: string | null; card_json: string | null;
  };

export type RatingSnapshotRow=SnapshotRow;

export type RatingMetersRow=MeterRow;

export type ProviderCostsRow=CostRow;

export type ProviderStatementsRow=StatementRow;

export type ApprovedExtrasRow={
    id: string; kind: "setup" | "support"; description: string;
    amount_satang: string; evidence_id: string;
  };

export type RatingSeatIntervalsRow={user_key:string;intervals_json:unknown;snapshot_id:string};

