import "server-only";

import type { PoolClient } from "pg";
import { monthBounds } from "@/lib/billing";
import { betaLimits } from "./beta-config";
import type { PostgresDatabase } from "./runtime/postgres";

const operatingCategories = ["infrastructure", "backup", "stripe", "support"] as const;

function previousMonth(month: string, count: number) {
  const bounds = monthBounds(month);
  return new Date(bounds.start - count * 86_400_000).toISOString().slice(0, 7);
}

export type TargetMarginReport = {
  month: string;
  ratedRevenueSatang: number | null;
  verifiedProviderAiCostSatang: number | null;
  verifiedOperatingCostSatang: number | null;
  grossMarginBps: number | null;
  missingCosts: string[];
};
export type TargetRolloutReadiness = {
  holdingId: string;
  effectiveMonth: string;
  minimumMarginBps: number;
  shadowMonths: [string, string];
  margins: TargetMarginReport[];
  capacity: {
    reviewed: boolean;
    projectedOriginalBytes: number | null;
    primaryAvailableBytes: number | null;
    backupAvailableBytes: number | null;
    reservedBytes: number | null;
    companyLimitBytes: number | null;
  };
  ready: boolean;
  blockers: string[];
};

export async function readTargetRolloutReadiness(
  db: PostgresDatabase,
  holdingId: string,
  effectiveMonth: string,
  minimumMarginBps: number,
  now = Date.now(),
): Promise<TargetRolloutReadiness> {
  const c = await db.pool.connect();
  try {
    return await calculateTargetRolloutReadiness(c, holdingId, effectiveMonth, minimumMarginBps, now);
  } finally {
    c.release();
  }
}

export async function calculateTargetRolloutReadiness(
  c: PoolClient,
  holdingId: string,
  effectiveMonth: string,
  minimumMarginBps: number,
  now: number,
): Promise<TargetRolloutReadiness> {
  monthBounds(effectiveMonth);
  if (!Number.isInteger(minimumMarginBps) || minimumMarginBps < 0 || minimumMarginBps > 10_000)
    throw new RangeError("INVALID_MARGIN_THRESHOLD");
  const second = previousMonth(effectiveMonth, 1);
  const first = previousMonth(effectiveMonth, 32);
  const shadowMonths: [string, string] = [first, second];
  const blockers: string[] = [];
  const margins: TargetMarginReport[] = [];
  const price = (await c.query<{
    id: string; included_original_bytes: string; seat_terms_status: string; rating_enabled: boolean;
  }>(`SELECT id,included_original_bytes,seat_terms_status,rating_enabled
      FROM billing_target_prices WHERE effective_month<=$1
      ORDER BY effective_month DESC,id DESC LIMIT 1`,
    [effectiveMonth])).rows[0];
  for (const month of shadowMonths) {
    const missingCosts: string[] = [];
    const rated = (await c.query<{
      state: string; mode: string; total_satang: string | null; price_id: string;
      meter_snapshot_id: string; lines_json: string;
    }>("SELECT state,mode,total_satang,price_id,meter_snapshot_id,lines_json FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2",
      [holdingId, month])).rows[0];
    const snapshot = (await c.query<{snapshot_id:string}>(
      "SELECT snapshot_id FROM billing_usage_snapshots WHERE holding_id=$1 AND month=$2",
      [holdingId, month])).rows[0];
    if (!rated || rated.mode !== "shadow" || rated.state !== "ready" || !snapshot ||
        rated.meter_snapshot_id !== snapshot.snapshot_id || now < monthBounds(month).end)
      blockers.push(`complete_shadow_cycle_missing:${month}`);
    if (rated && (!price || rated.price_id !== price.id))
      blockers.push(`shadow_price_version_mismatch:${month}`);
    const operatingRows = (await c.query<{
      category: string; amount_satang: string; source_state: string;
    }>(`SELECT o.category,o.amount_satang,e.source_state
       FROM billing_target_operating_costs o
       JOIN billing_evidence e ON e.id=o.evidence_id
       WHERE o.holding_id=$1 AND o.month=$2`, [holdingId, month])).rows;
    for (const category of operatingCategories)
      if (!operatingRows.some(row => row.category === category && row.source_state === "available"))
        missingCosts.push(category);
    if (missingCosts.length) blockers.push(`operating_costs_missing:${month}:${missingCosts.join(",")}`);
    let providerAiCost: number | null = null;
    if (rated && rated.state === "ready") {
      const lines = JSON.parse(rated.lines_json) as {
        kind: string; sourceKey: string | null; fundingSource?: string | null;
        amountSatang: number | null; verifiedProviderCostSatang: number | null;
      }[];
      const ai = lines.filter(line => line.kind === "ai" && line.fundingSource === "erpx" && line.amountSatang !== 0);
      if (ai.every(line => line.verifiedProviderCostSatang !== null))
        providerAiCost = ai.reduce((sum, line) => sum + (line.verifiedProviderCostSatang ?? 0), 0);
    }
    if (providerAiCost === null) blockers.push(`provider_costs_unavailable:${month}`);
    const operatingCost = missingCosts.length === 0
      ? operatingRows.reduce((sum, row) => sum + Number(row.amount_satang), 0)
      : null;
    const revenue = rated?.state === "ready" && rated.total_satang !== null
      ? Number(rated.total_satang) : null;
    const margin = revenue !== null && revenue > 0 && operatingCost !== null && providerAiCost !== null
      ? Number((BigInt(revenue - operatingCost - providerAiCost) * BigInt(10_000)) / BigInt(revenue))
      : null;
    if (margin === null) blockers.push(`margin_unavailable:${month}`);
    else if (margin < minimumMarginBps) blockers.push(`margin_below_threshold:${month}`);
    margins.push({ month, ratedRevenueSatang: revenue,
      verifiedProviderAiCostSatang: providerAiCost,
      verifiedOperatingCostSatang: operatingCost,
      grossMarginBps: margin, missingCosts });
  }
  const capacityRow = (await c.query<{
    projected_original_bytes: string; primary_available_bytes: string;
    backup_available_bytes: string; reserved_bytes: string;
    company_limit_bytes: string; source_state: string;
  }>(`SELECT r.*,e.source_state FROM billing_target_capacity_reviews r
     JOIN billing_evidence e ON e.id=r.evidence_id
     WHERE r.holding_id=$1 AND r.effective_month=$2`,
    [holdingId, effectiveMonth])).rows[0];
  const capacity = {
    reviewed: Boolean(capacityRow && capacityRow.source_state === "available"),
    projectedOriginalBytes: capacityRow ? Number(capacityRow.projected_original_bytes) : null,
    primaryAvailableBytes: capacityRow ? Number(capacityRow.primary_available_bytes) : null,
    backupAvailableBytes: capacityRow ? Number(capacityRow.backup_available_bytes) : null,
    reservedBytes: capacityRow ? Number(capacityRow.reserved_bytes) : null,
    companyLimitBytes: capacityRow ? Number(capacityRow.company_limit_bytes) : null,
  };
  if (!capacity.reviewed) blockers.push("capacity_review_missing");
  if (!price) blockers.push("target_price_missing");
  if (price?.seat_terms_status !== "defined") blockers.push("seat_terms_pending");
  if (!price?.rating_enabled) blockers.push("price_version_provisional");
  if (capacityRow && price) {
    const needed = BigInt(capacityRow.projected_original_bytes);
    const reserve = BigInt(capacityRow.reserved_bytes);
    if (BigInt(capacityRow.primary_available_bytes) - reserve < needed ||
        BigInt(capacityRow.backup_available_bytes) - reserve < needed)
      blockers.push("primary_or_backup_capacity_insufficient");
    if (BigInt(capacityRow.company_limit_bytes) < BigInt(price.included_original_bytes) ||
        BigInt(betaLimits({ BETA_STORAGE_BYTES: process.env.BETA_STORAGE_BYTES }).storage) < BigInt(price.included_original_bytes))
      blockers.push("company_storage_cap_not_raised");
  }
  return { holdingId, effectiveMonth, minimumMarginBps, shadowMonths,
    margins, capacity, ready: blockers.length === 0, blockers };
}
