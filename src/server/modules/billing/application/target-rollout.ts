import { monthBounds } from "@/lib/billing/billing";
import type {TargetMarginReport,TargetRolloutReadiness,EffectivePriceRow,RatedShadowPeriodRow,UsageSnapshotRow,AvailableOperatingCostsRow,CapacityReviewRow} from '../domain/target-rollout';

export interface TargetRolloutRepository{effectivePrice(effectiveMonth:string):Promise<{rows:EffectivePriceRow[];rowCount:number|null}>;
ratedShadowPeriod(holdingId:string,month:string):Promise<{rows:RatedShadowPeriodRow[];rowCount:number|null}>;
usageSnapshot(holdingId:string,month:string):Promise<{rows:UsageSnapshotRow[];rowCount:number|null}>;
availableOperatingCosts(holdingId:string,month:string):Promise<{rows:AvailableOperatingCostsRow[];rowCount:number|null}>;
capacityReview(holdingId:string,effectiveMonth:string):Promise<{rows:CapacityReviewRow[];rowCount:number|null}>;}
const operatingCategories = ["infrastructure", "backup", "stripe", "support"] as const;

function previousMonth(month: string, count: number) {
  const bounds = monthBounds(month);
  return new Date(bounds.start - count * 86_400_000).toISOString().slice(0, 7);
}

export async function calculateTargetRolloutReadiness(
  c: TargetRolloutRepository,
  holdingId: string,
  effectiveMonth: string,
  minimumMarginBps: number,
  now: number,
  storageLimit: number,
): Promise<TargetRolloutReadiness> {
  monthBounds(effectiveMonth);
  if (!Number.isInteger(minimumMarginBps) || minimumMarginBps < 0 || minimumMarginBps > 10_000)
    throw new RangeError("INVALID_MARGIN_THRESHOLD");
  const second = previousMonth(effectiveMonth, 1);
  const first = previousMonth(effectiveMonth, 32);
  const shadowMonths: [string, string] = [first, second];
  const blockers: string[] = [];
  const margins: TargetMarginReport[] = [];
  const price = (await c.effectivePrice(effectiveMonth)).rows[0];
  for (const month of shadowMonths) {
    const missingCosts: string[] = [];
    const rated = (await c.ratedShadowPeriod(holdingId,month)).rows[0];
    const snapshot = (await c.usageSnapshot(holdingId,month)).rows[0];
    if (!rated || rated.mode !== "shadow" || rated.state !== "ready" || !snapshot ||
        rated.meter_snapshot_id !== snapshot.snapshot_id || now < monthBounds(month).end)
      blockers.push(`complete_shadow_cycle_missing:${month}`);
    if (rated && (!price || rated.price_id !== price.id))
      blockers.push(`shadow_price_version_mismatch:${month}`);
    const operatingRows = (await c.availableOperatingCosts(holdingId,month)).rows;
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
  const capacityRow = (await c.capacityReview(holdingId,effectiveMonth)).rows[0];
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
        BigInt(storageLimit) < BigInt(price.included_original_bytes))
      blockers.push("company_storage_cap_not_raised");
  }
  return { holdingId, effectiveMonth, minimumMarginBps, shadowMonths,
    margins, capacity, ready: blockers.length === 0, blockers };
}
