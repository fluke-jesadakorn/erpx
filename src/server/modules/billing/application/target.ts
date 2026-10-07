import { billingMonth, monthBounds } from "@/lib/billing";
import {
  rateTargetAi,
  rateTargetBase,
  rateTargetStorage,
  rateTargetSeats,
  targetServiceWindow,
  unionSeatIntervals,
  type TargetPrice,
} from "@/lib/billing-target-pricing";
import type {PriceRow,TargetMeasuredEvidence,MeterRow,TargetRatedLine,TargetBillingPeriodView,OperatingCostsRow,MeasuredSnapshotRow,CurrentSeatsRow,MeasuredMetersRow,MeasuredSeatIntervalsRow,AssignedOfferingRow,AssignedPriceRow,ApprovedRatingRow,ApprovedInvoiceRow,ApprovedStatementsRow,RolloutEligibilityRow,LegacyEnrollmentOverlapRow,MonthInvoicesRow,PaymentConsentRow,RatingSnapshotRow,RatingMetersRow,ProviderCostsRow,ProviderStatementsRow,ApprovedExtrasRow,RatingSeatIntervalsRow} from '../domain/target';

export interface TargetBillingRepository{operatingCosts(holdingId:string,month:string):Promise<{rows:OperatingCostsRow[];rowCount:number|null}>;
measuredSnapshot(holdingId:string,month:string):Promise<{rows:MeasuredSnapshotRow[];rowCount:number|null}>;
currentSeats(holdingId:string,billingMonth:string):Promise<{rows:CurrentSeatsRow[];rowCount:number|null}>;
measuredMeters(holdingId:string,month:string):Promise<{rows:MeasuredMetersRow[];rowCount:number|null}>;
measuredSeatIntervals(holdingId:string,month:string):Promise<{rows:MeasuredSeatIntervalsRow[];rowCount:number|null}>;
assignment(holdingId:string,month:string):Promise<{rows:AssignedOfferingRow[];rowCount:number|null}>;
assignedPrice(price_id:string,month:string):Promise<{rows:AssignedPriceRow[];rowCount:number|null}>;
approvedRating(holdingId:string,month:string):Promise<{rows:ApprovedRatingRow[];rowCount:number|null}>;
approvedInvoice(invoice_id:string):Promise<{rows:ApprovedInvoiceRow[];rowCount:number|null}>;
approvedStatements(holdingId:string,month:string):Promise<{rows:ApprovedStatementsRow[];rowCount:number|null}>;
rolloutEligibility(holdingId:string,month:string):Promise<{rows:RolloutEligibilityRow[];rowCount:number|null}>;
legacyEnrollmentOverlap(holdingId:string,start:number,end:number):Promise<{rows:LegacyEnrollmentOverlapRow[];rowCount:number|null}>;
monthInvoices(holdingId:string,month:string):Promise<{rows:MonthInvoicesRow[];rowCount:number|null}>;
paymentConsent(holdingId:string):Promise<{rows:PaymentConsentRow[];rowCount:number|null}>;
ratingSnapshot(holdingId:string,month:string):Promise<{rows:RatingSnapshotRow[];rowCount:number|null}>;
ratingMeters(holdingId:string,month:string):Promise<{rows:RatingMetersRow[];rowCount:number|null}>;
providerCosts(holdingId:string,month:string):Promise<{rows:ProviderCostsRow[];rowCount:number|null}>;
providerStatements(holdingId:string,month:string):Promise<{rows:ProviderStatementsRow[];rowCount:number|null}>;
approvedExtras(holdingId:string,month:string):Promise<{rows:ApprovedExtrasRow[];rowCount:number|null}>;
ratingSeatIntervals(holdingId:string,month:string):Promise<{rows:RatingSeatIntervalsRow[];rowCount:number|null}>;}
function number(value: string | number, field: string): number {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 0) throw new RangeError(`INVALID_${field}`);
  return parsed;
}

function missingOnce(items: string[], key: string) {
  if (!items.includes(key)) items.push(key);
}

function priceFrom(row: PriceRow): TargetPrice {
  return {
    id: row.id,
    effectiveMonth: row.effective_month,
    baseMonthlySatang: number(row.base_monthly_satang, "BASE"),
    includedOriginalBytes: number(row.included_original_bytes, "INCLUDED_BYTES"),
    storageSatangPerGbMonth: number(row.storage_satang_per_gb_month, "STORAGE_RATE"),
    aiMarkupPercent: number(row.ai_markup_percent, "AI_MARKUP"),
    seatTermsStatus: row.seat_terms_status,
    includedSeats: row.included_seats,
    seatMonthlySatang: row.seat_monthly_satang === null ? null : number(row.seat_monthly_satang, "SEAT_RATE"),
    storageAllowanceMode: row.storage_allowance_mode,
    ratingEnabled: row.rating_enabled,
  };
}

function safeSum(values: readonly (string | number)[], field: string): number {
  const total = values.reduce((sum, value) => sum + BigInt(value), BigInt(0));
  if (total > BigInt(Number.MAX_SAFE_INTEGER)) throw new RangeError(`INVALID_${field}`);
  return Number(total);
}

function providerUnits(value: MeterRow["units_json"]): Record<string, number> | null {
  return typeof value === "string" ? JSON.parse(value) as Record<string, number> : value;
}

async function readMeasuredEvidence(
  c: TargetBillingRepository, holdingId: string, month: string, now: number,
): Promise<TargetMeasuredEvidence> {
  const operatingRows = (await c.operatingCosts(holdingId,month)).rows;
  const categories = Object.fromEntries(
    (["infrastructure", "backup", "stripe", "support"] as const).map(category => {
      const evidence = operatingRows.find(row => row.category === category && row.source_state === "available");
      return [category, evidence
        ? { status: "verified" as const, amountSatang: number(evidence.amount_satang, "OPERATING_COST") }
        : { status: "unavailable" as const, amountSatang: null }];
    }),
  ) as TargetMeasuredEvidence["operatingCosts"]["categories"];
  const operatingCosts: TargetMeasuredEvidence["operatingCosts"] = {
    categories,
    totalVerifiedSatang: Object.values(categories).every(item => item.status === "verified")
      ? safeSum(Object.values(categories).map(item => item.amountSatang!), "OPERATING_COST_TOTAL")
      : null,
  };
  const snapshot = (await c.measuredSnapshot(holdingId,month)).rows[0];
  const currentSnapshot = (await c.currentSeats(holdingId,billingMonth(now))).rows[0];
  const activeNow = currentSnapshot?.active_seat_count !== null &&
    currentSnapshot?.active_seat_count !== undefined &&
    currentSnapshot.active_seat_observed_at !== null
    ? { status: "measured" as const, count: currentSnapshot.active_seat_count,
      asOf: number(currentSnapshot.active_seat_observed_at, "SEAT_OBSERVED_AT") }
    : { status: "unavailable" as const, count: null, asOf: null };
  const unavailable: TargetMeasuredEvidence = {
    snapshot: null,
    ai: { requests: null, inputTokens: null, outputTokens: null,
      platformSucceeded: 0, platformVerified: 0, platformPending: 0,
      byok: 0, failed: 0, simulation: 0, unknown: 0,
      verifiedProviderCostSatang: null, chargeableVerifiedProviderCostSatang: null,
      totalProviderCostSatang: null, sources: [] },
    storage: { originalCount: null, activeBytesAtEnd: null,
      byteMilliseconds: null, coverage: "unavailable" },
    seats: { distinctUsers: null, activeSeatMilliseconds: null,
      coverage: "unavailable", activeNow },
    operatingCosts,
  };
  if (!snapshot) return unavailable;
  const meters = (await c.measuredMeters(holdingId,month)).rows;
  const seatRows = (await c.measuredSeatIntervals(holdingId,month)).rows;
  const aiRows = meters.filter(row => row.meter_kind === "ai");
  const storageRows = meters.filter(row => row.meter_kind === "storage");
  const aiSources: TargetMeasuredEvidence["ai"]["sources"] = [];
  const verifiedCosts: number[] = [];
  const chargeableVerifiedCosts: number[] = [];
  let platformSucceeded = 0;
  let platformVerified = 0;
  let platformPending = 0;
  let byok = 0;
  let failed = 0;
  let simulation = 0;
  let unknown = 0;
  for (const row of aiRows) {
    const validSuccess = row.request_status === "succeeded" && row.gateway_status === "succeeded";
    const reconciled = row.cost_status === "reconciled" &&
      row.cost_reconciled_at !== null && row.verified_cost_satang !== null;
    const billableStatus = row.funding_source === "unknown" || row.gateway_status === null ||
      row.gateway_status === "reserved" ||
      (row.request_status === "succeeded" && row.gateway_status !== "succeeded")
      ? "price_unavailable" as const
      : row.request_status === "failed" ? "excluded_failed" as const
      : row.funding_source === "simulation" ? "simulation" as const
      : row.funding_source === "byok" ? "byok" as const
      : row.funding_source === "erpx" && validSuccess && reconciled
        ? "verified_cost" as const : "price_unavailable" as const;
    if (row.funding_source === "unknown") unknown++;
    if (row.request_status === "failed") failed++;
    if (row.funding_source === "simulation") simulation++;
    if (row.funding_source === "byok") byok++;
    if (row.funding_source === "erpx" && reconciled)
      verifiedCosts.push(number(row.verified_cost_satang!, "PROVIDER_COST"));
    if (row.funding_source === "erpx" && validSuccess) {
      platformSucceeded++;
      if (billableStatus === "verified_cost") {
        platformVerified++;
        chargeableVerifiedCosts.push(number(row.verified_cost_satang!, "PROVIDER_COST"));
      } else platformPending++;
    }
    aiSources.push({
      sourceKey: row.source_key, companyId: row.company_id, branchId: row.branch_id,
      provider: row.provider, fundingSource: row.funding_source,
      requestStatus: row.request_status, gatewayStatus: row.gateway_status,
      providerUnits: providerUnits(row.units_json), costStatus: row.cost_status,
      verifiedProviderCostSatang: row.funding_source === "erpx" && reconciled
        ? number(row.verified_cost_satang!, "PROVIDER_COST") : null,
      costEvidenceKey: row.cost_evidence_key, billableStatus,
    });
  }
  const sourceCountMatches = meters.length === snapshot.source_count &&
    meters.every(row => row.snapshot_id === snapshot.snapshot_id);
  const aiComplete = snapshot.ai_coverage === "measured" && sourceCountMatches &&
    aiSources.every(source => source.billableStatus !== "price_unavailable" &&
      (source.fundingSource !== "erpx" || source.verifiedProviderCostSatang !== null ||
        (source.gatewayStatus === "failed" && source.costStatus === "not_applicable")));
  const verifiedProviderCostSatang = safeSum(verifiedCosts, "PROVIDER_COST_TOTAL");
  const chargeableVerifiedProviderCostSatang = safeSum(chargeableVerifiedCosts, "CHARGEABLE_PROVIDER_COST_TOTAL");
  let byteMilliseconds: string | null = null;
  if (snapshot.storage_coverage === "measured" && snapshot.unreconciled_originals === 0 &&
      sourceCountMatches && storageRows.every(row => row.byte_milliseconds !== null)) {
    byteMilliseconds = storageRows.reduce((sum, row) =>
      sum + BigInt(row.byte_milliseconds!), BigInt(0)).toString();
  }
  let activeSeatMilliseconds: string | null = null;
  if (snapshot.seat_coverage === "measured" &&
      seatRows.length === snapshot.seat_source_count &&
      seatRows.every(row => row.snapshot_id === snapshot.snapshot_id)) {
    try {
      const bounds = monthBounds(month);
      let total = BigInt(0);
      for (const row of seatRows) {
        const intervals = typeof row.intervals_json === "string"
          ? JSON.parse(row.intervals_json) as unknown : row.intervals_json;
        if (!Array.isArray(intervals) || !intervals.every(value =>
          Array.isArray(value) && value.length === 2 &&
          Number.isSafeInteger(value[0]) && Number.isSafeInteger(value[1])))
          throw new RangeError("INVALID_SEAT_INTERVALS");
        for (const [start, end] of unionSeatIntervals(intervals as [number, number][]))
          total += BigInt(Math.max(0, Math.min(end, bounds.end) - Math.max(start, bounds.start)));
      }
      activeSeatMilliseconds = total.toString();
    } catch { /* Invalid evidence must remain unavailable, never imply zero seats. */ }
  }
  return {
    snapshot: { id: snapshot.snapshot_id, refreshedAt: number(snapshot.refreshed_at, "REFRESHED_AT"),
      sourceCount: snapshot.source_count, seatSourceCount: snapshot.seat_source_count,
      aiCoverage: snapshot.ai_coverage, storageCoverage: snapshot.storage_coverage,
      seatCoverage: snapshot.seat_coverage },
    ai: { requests: aiRows.length,
      inputTokens: safeSum(aiRows.map(row => row.input_tokens), "INPUT_TOKENS"),
      outputTokens: safeSum(aiRows.map(row => row.output_tokens), "OUTPUT_TOKENS"),
      platformSucceeded, platformVerified, platformPending, byok, failed, simulation, unknown,
      verifiedProviderCostSatang, chargeableVerifiedProviderCostSatang,
      totalProviderCostSatang: aiComplete ? verifiedProviderCostSatang : null,
      sources: aiSources },
    storage: { originalCount: storageRows.length,
      activeBytesAtEnd: safeSum(storageRows.filter(row => row.active_at_period_end)
        .map(row => row.quantity), "ACTIVE_BYTES"),
      byteMilliseconds,
      coverage: byteMilliseconds === null ? "partial" : "measured" },
    seats: { distinctUsers: activeSeatMilliseconds === null ? null : seatRows.length,
      activeSeatMilliseconds,
      coverage: activeSeatMilliseconds === null ? "unavailable" : "measured",
      activeNow },
    operatingCosts,
  };
}

export async function calculateTargetBillingPeriod(
  c: TargetBillingRepository,
  holdingId: string,
  month: string,
  now: number,
): Promise<TargetBillingPeriodView> {
  const bounds = monthBounds(month);
  const measuredEvidence = await readMeasuredEvidence(c, holdingId, month, now);
  const assignment = (await c.assignment(holdingId,month)).rows[0];
  const priceRow = assignment ? (await c.assignedPrice(assignment.price_id,month)).rows[0] : null;
  const price = priceRow ? priceFrom(priceRow) : null;
  const mappedAssignment = assignment ? {
    effectiveMonth: assignment.effective_month,
    mode: assignment.mode,
    priceId: assignment.price_id,
    serviceStartedAt: number(assignment.service_started_at, "SERVICE_START"),
  } : null;
  const approved = (await c.approvedRating(holdingId,month)).rows[0];
  if (approved) {
    const invoice = (await c.approvedInvoice(approved.invoice_id)).rows[0];
    const approvedStatements = (await c.approvedStatements(holdingId,month)).rows;
    const lines = JSON.parse(approved.lines_json) as TargetRatedLine[];
    return {
      holdingId, month, assignment: mappedAssignment, price, measuredEvidence,
      completeness: { ready: true, missing: [], meterSnapshotId: approved.meter_snapshot_id },
      approval: { eligible: false, blockers: ["already_approved"] },
      lines, sources: lines.filter(line => line.sourceKey !== null),
      statementReconciliation: approvedStatements.map(row => ({
        provider: row.provider, statementId: row.statement_id,
        originalCurrency: row.original_currency,
        providerTotalMinor: row.provider_total_minor,
        reconciledTotalMinor: row.reconciled_total_minor,
        status: row.provider_total_minor === row.reconciled_total_minor ? "verified" : "mismatch",
      })),
      totalSatang: number(approved.total_satang, "TOTAL"),
      invoice: invoice ? { id: invoice.id, totalSatang: number(invoice.total_satang, "INVOICE_TOTAL"),
        status: invoice.status, stripeId: invoice.stripe_id } : null,
      state: "approved",
    };
  }
  const missing: string[] = [];
  const lines: TargetRatedLine[] = [];
  const empty: TargetBillingPeriodView = {
    holdingId, month, assignment: mappedAssignment, price, measuredEvidence,
    completeness: { ready: false, missing, meterSnapshotId: null },
    approval: { eligible: false, blockers: [] },
    lines, sources: [], statementReconciliation: [], totalSatang: null,
    invoice: null, state: assignment ? "held" : "unassigned",
  };
  if (!assignment || !price) {
    empty.completeness.meterSnapshotId = measuredEvidence.snapshot?.id ?? null;
    missing.push(assignment ? "price_version_unavailable" : "target_offering_not_assigned");
    empty.approval.blockers = [...missing];
    return empty;
  }
  if (price.seatTermsStatus !== "defined") missingOnce(missing, "seat_terms_pending");
  if (!price.ratingEnabled) missingOnce(missing, "price_version_provisional");
  const approvalBlockers: string[] = [];
  if (assignment.mode !== "sandbox_invoice") approvalBlockers.push("shadow_only");
  if (!(await c.rolloutEligibility(holdingId,month)).rowCount) approvalBlockers.push("sandbox_designation_or_rollout_approval_missing");
  if ((await c.legacyEnrollmentOverlap(holdingId,bounds.start,bounds.end)).rowCount) approvalBlockers.push("grandfathered_support_active");
  if ((await c.monthInvoices(holdingId,month)).rowCount) approvalBlockers.push("month_already_invoiced");
  const payment = (await c.paymentConsent(holdingId)).rows[0];
  if (payment?.payment_mode === "auto" && (!payment.consent_at || !payment.card_json))
    approvalBlockers.push("automatic_payment_consent_missing");
  empty.approval.blockers = approvalBlockers;
  if (now < bounds.end) missingOnce(missing, "month_not_closed");
  const service = targetServiceWindow(month, Math.max(bounds.start, Number(assignment.service_started_at)));
  const snapshot = (await c.ratingSnapshot(holdingId,month)).rows[0];
  if (!snapshot) {
    missingOnce(missing, "meter_snapshot_missing");
    empty.approval.blockers = [...approvalBlockers, ...missing];
    return empty;
  }
  empty.completeness.meterSnapshotId = snapshot.snapshot_id;
  if (snapshot.ai_coverage !== "measured") missingOnce(missing, "ai_meter_incomplete");
  if (snapshot.seat_coverage !== "measured") missingOnce(missing, "seat_meter_pending");
  if (snapshot.storage_coverage !== "measured" || snapshot.unreconciled_originals !== 0)
    missingOnce(missing, "storage_meter_incomplete");
  // The exact NUMERIC(38,0) byte-time column exceeds JS safe integers. Read
  // original bytes and lifetime instead, and keep all arithmetic in bigint.
  const meterRows = await c.ratingMeters(holdingId,month);
  const costRows = await c.providerCosts(holdingId,month);
  const statementRows = await c.providerStatements(holdingId,month);
  const extras = (await c.approvedExtras(holdingId,month)).rows;
  const meters = meterRows.rows;
  if (meters.length !== snapshot.source_count) missingOnce(missing, "meter_count_mismatch");
  const costs = new Map(costRows.rows.map(row => [row.source_key, row]));
  const statements = new Map(statementRows.rows.map(row => [`${row.provider}\0${row.statement_id}\0${row.original_currency}`, row]));
  const meterKeys = new Set(meters.map(row => row.source_key));
  for (const cost of costRows.rows)
    if (!meterKeys.has(cost.source_key))
      missingOnce(missing, `orphan_provider_cost:${cost.source_key}`);
  const reconciliation = statementRows.rows.map(row => ({
    provider: row.provider, statementId: row.statement_id,
    originalCurrency: row.original_currency,
    providerTotalMinor: row.provider_total_minor,
    reconciledTotalMinor: row.reconciled_total_minor,
    status: row.provider_total_minor === row.reconciled_total_minor ? "verified" as const : "mismatch" as const,
  }));
  empty.statementReconciliation = reconciliation;
  let byteMilliseconds = BigInt(0);
  let paidSeatMilliseconds = BigInt(0);
  let seatAmount: number | null = null;
  const seatRows = (await c.ratingSeatIntervals(holdingId,month)).rows;
  if (seatRows.length !== snapshot.seat_source_count ||
      seatRows.some(row => row.snapshot_id !== snapshot.snapshot_id))
    missingOnce(missing, "seat_source_count_mismatch");
  if (snapshot.seat_coverage === "measured" && price.seatTermsStatus === "defined" &&
      price.includedSeats === 0 && price.seatMonthlySatang !== null) {
    try {
      const users = seatRows.map(row => {
        const parsed = typeof row.intervals_json === "string"
          ? JSON.parse(row.intervals_json) as unknown : row.intervals_json;
        if (!Array.isArray(parsed) || !parsed.every(value =>
          Array.isArray(value) && value.length === 2 &&
          Number.isSafeInteger(value[0]) && Number.isSafeInteger(value[1])))
          throw new RangeError("INVALID_SEAT_INTERVALS");
        return parsed as [number, number][];
      });
      const seatRate = rateTargetSeats(price, month, service.exactStart, users);
      paidSeatMilliseconds = seatRate.activeSeatMilliseconds;
      seatAmount = seatRate.amountSatang;
      for (let index = 0; index < seatRows.length; index++) {
        const individual = rateTargetSeats(price, month, service.exactStart, [users[index]]);
        lines.push({ kind: "seat", description: "Active named seat time", amountSatang: null,
          sourceKey: seatRows[index].user_key,
          measuredQuantity: `${individual.activeSeatMilliseconds} seat-ms`,
          verifiedProviderCostSatang: null, evidenceId: null, statementId: null,
          reconciliationStatus: "not_required", provider: null, fundingSource: null,
          requestStatus: null, providerUnits: null });
      }
    } catch {
      missingOnce(missing, "seat_intervals_invalid");
    }
  }
  lines.unshift({ kind: "seat", description: "Active named seats · monthly time", amountSatang: seatAmount,
    sourceKey: null,
    measuredQuantity: `${paidSeatMilliseconds}/${BigInt(bounds.end - bounds.start)} seat-months`,
    verifiedProviderCostSatang: null, evidenceId: null, statementId: null,
    reconciliationStatus: "not_required", provider: null, fundingSource: null,
    requestStatus: null, providerUnits: null });
  let storageSources = 0;
  for (const meter of meters) {
    if (meter.meter_kind !== "storage") continue;
    storageSources += 1;
    if (meter.active_from === null || meter.active_until === null) {
      missingOnce(missing, `storage_lifetime_missing:${meter.source_key}`);
      continue;
    }
    const activeFrom = number(meter.active_from, "ACTIVE_FROM");
    const activeUntil = number(meter.active_until, "ACTIVE_UNTIL");
    if (activeUntil < activeFrom) {
      missingOnce(missing, `storage_lifetime_invalid:${meter.source_key}`);
      continue;
    }
    const overlap = Math.max(0, Math.min(service.end, activeUntil) - Math.max(service.exactStart, activeFrom));
    byteMilliseconds += BigInt(number(meter.quantity, "ORIGINAL_BYTES")) * BigInt(overlap);
    lines.push({
      kind: "storage", description: "Original file byte-time", amountSatang: null,
      sourceKey: meter.source_key, measuredQuantity: String(BigInt(number(meter.quantity, "ORIGINAL_BYTES")) * BigInt(overlap)),
      verifiedProviderCostSatang: null, evidenceId: null, statementId: null,
      reconciliationStatus: "not_required", provider: null, fundingSource: null,
      requestStatus: null,
      providerUnits: null,
    });
  }
  const baseAmount = rateTargetBase(price, month, service.start);
  const storageComplete = snapshot.storage_coverage === "measured" &&
    snapshot.unreconciled_originals === 0 &&
    !missing.some(value => value.startsWith("storage_lifetime_"));
  const storageAmount = storageComplete &&
    (price.storageAllowanceMode === "holding_flat" || seatAmount !== null)
    ? rateTargetStorage(price, month, service.exactStart, byteMilliseconds, paidSeatMilliseconds) : null;
  lines.unshift({ kind: "storage", description: `Original storage over ${price.includedOriginalBytes} included bytes`,
    amountSatang: storageAmount, sourceKey: null, measuredQuantity: byteMilliseconds.toString(),
    verifiedProviderCostSatang: null, evidenceId: null, statementId: null,
    reconciliationStatus: "not_required", provider: null, fundingSource: null,
    requestStatus: null, providerUnits: null });
  lines.unshift({ kind: "base", description: "Holding platform monthly fee", amountSatang: baseAmount,
    sourceKey: null, measuredQuantity: String(service.serviceDays),
    verifiedProviderCostSatang: null, evidenceId: null, statementId: null,
    reconciliationStatus: "not_required", provider: null, fundingSource: null,
    requestStatus: null, providerUnits: null });
  const seenStatement = new Map<string, bigint>();
  for (const cost of costRows.rows) {
    const key = `${cost.provider}\0${cost.statement_id}\0${cost.original_currency}`;
    seenStatement.set(key, (seenStatement.get(key) ?? BigInt(0)) + BigInt(cost.original_minor));
  }
  for (const meter of meters) {
    if (meter.meter_kind !== "ai") continue;
    const sourceKey = meter.source_key;
    const status = meter.request_status;
    if (meter.funding_source === "unknown" || meter.gateway_status === null ||
        (status === "succeeded" && meter.gateway_status !== "succeeded") ||
        (status === "failed" && !["failed", "succeeded"].includes(meter.gateway_status)))
      missingOnce(missing, `ai_gateway_attribution_missing:${sourceKey}`);
    const excluded = status === "failed" || meter.funding_source === "simulation" || status === "simulation";
    const byok = meter.funding_source === "byok";
    let amountSatang: number | null = excluded || byok ? 0 : null;
    let verifiedCost: number | null = null;
    let evidenceId: string | null = null;
    let statementId: string | null = null;
    let reconciliationStatus: TargetRatedLine["reconciliationStatus"] = excluded || byok ? "not_required" : "missing";
    if (!excluded && !byok) {
      if (status !== "succeeded" || meter.funding_source !== "erpx")
        missingOnce(missing, `ai_attribution_missing:${sourceKey}`);
      else {
        const cost = costs.get(sourceKey);
        const statement = cost ? statements.get(`${cost.provider}\0${cost.statement_id}\0${cost.original_currency}`) : null;
        const matchingMeter = meter.cost_status === "reconciled" && meter.cost_reconciled_at !== null &&
          meter.verified_cost_satang !== null && cost &&
          cost.provider_cost_satang === meter.verified_cost_satang &&
          cost.provider === meter.provider;
        if (!cost || !matchingMeter) missingOnce(missing, `provider_cost_missing:${sourceKey}`);
        else if (!statement)
          missingOnce(missing, `statement_unverified:${sourceKey}`);
        else if (statement.original_currency !== cost.original_currency ||
          statement.provider_total_minor !== statement.reconciled_total_minor)
          missingOnce(missing, `statement_mismatch:${sourceKey}`);
        else {
          verifiedCost = number(cost.provider_cost_satang, "PROVIDER_COST");
          amountSatang = rateTargetAi(price, verifiedCost);
          evidenceId = cost.evidence_id;
          statementId = cost.statement_id;
          reconciliationStatus = "verified";
        }
      }
    }
    lines.push({
      kind: "ai", description: excluded ? "AI request excluded" : byok ? "Customer-paid AI (BYOK)" : "Platform-paid AI usage",
      amountSatang, sourceKey,
      measuredQuantity: `${meter.input_tokens}+${meter.output_tokens} tokens`,
      verifiedProviderCostSatang: verifiedCost, evidenceId, statementId,
      reconciliationStatus, provider: meter.provider, fundingSource: meter.funding_source,
      requestStatus: status,
      providerUnits: typeof meter.units_json === "string"
        ? JSON.parse(meter.units_json) as Record<string, number>
        : meter.units_json,
    });
  }
  for (const [key, sum] of seenStatement) {
    const statement = statements.get(key);
    if (!statement || BigInt(statement.provider_total_minor) !== sum)
      missingOnce(missing, `statement_source_sum_mismatch:${key.replaceAll("\0", ":")}`);
  }
  for (const extra of extras) lines.push({
    kind: "extra", description: `Approved ${extra.kind}: ${extra.description}`,
    amountSatang: number(extra.amount_satang, "EXTRA_AMOUNT"),
    sourceKey: extra.id, measuredQuantity: null,
    verifiedProviderCostSatang: null, evidenceId: extra.evidence_id,
    statementId: null, reconciliationStatus: "not_required",
    provider: null, fundingSource: null, requestStatus: null,
    providerUnits: null,
  });
  if (storageSources !== meters.filter(meter => meter.meter_kind === "storage").length)
    missingOnce(missing, "storage_source_mismatch");
  empty.sources = lines.filter(line => line.sourceKey !== null);
  empty.completeness.ready = missing.length === 0;
  empty.approval.blockers = [...approvalBlockers, ...missing];
  empty.approval.eligible = empty.approval.blockers.length === 0;
  empty.state = missing.length === 0 ? "ready" : "held";
  if (missing.length === 0) {
    const total = lines.reduce((sum, line) => sum + BigInt(line.amountSatang ?? 0), BigInt(0));
    if (total > BigInt(Number.MAX_SAFE_INTEGER)) throw new RangeError("TARGET_TOTAL_OUT_OF_RANGE");
    empty.totalSatang = Number(total);
  }
  return empty;
}
