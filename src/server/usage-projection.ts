import "server-only";

import { createHash, randomUUID } from "node:crypto";
import { billingMonth, monthBounds } from "@/lib/billing";
import { previewSandboxStorage, sandboxPricingForMonth } from "@/lib/billing-pricing";
import { holdingResources } from "./runtime";
import { isTransactionalSqlDatabase, type SqlDatabase } from "./runtime/types";
import { readHoldingUsage, type UsageCompany, type UsageHolding, type UsageItem, type UsageMetrics } from "./usage";
import { canonicalSeatIntervals, readControlSeatSummary, type HoldingSeatMeter, type SeatInterval } from "./seat-meter";

type Binding = { id: string; slug: string; displayName: string };
type Snapshot = { holding: UsageHolding; items: UsageItem[]; seats?: HoldingSeatMeter };
type Coverage = UsageHolding["meterCoverage"];
type SnapshotHeader = { holding_id: string; ai_coverage: Coverage["ai"];
  storage_coverage: Coverage["storage"]; seat_coverage: Coverage["seat"];
  seat_source_count: number; unreconciled_originals: number;
  active_seat_count:number|null; active_seat_observed_at:number|null };
type MeterRow = {
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

const meterKey = (holdingId: string, month: string, item: UsageItem) =>
  createHash("sha256").update(`${holdingId}\0${month}\0${item.type}\0${item.id}`).digest("hex");
const seatKey = (holdingId:string,month:string,userId:string) =>
  createHash("sha256").update(`billing-seat-v1\0${holdingId}\0${month}\0${userId}`).digest("hex");

async function principalSeatIntervals(control:SqlDatabase,holdingId:string,month:string,seats:HoldingSeatMeter):Promise<{coverage:"measured"|"unavailable";rows:{userKey:string;intervals:SeatInterval[]}[]}> {
  if(seats.coverage!=="measured")return {coverage:"unavailable",rows:[]};
  const {start,end}=monthBounds(month);
  const coverage=await control.prepare("SELECT since_at FROM billing_principal_status_coverage WHERE singleton=TRUE")
    .first<{since_at:number}>();
  if(!coverage||coverage.since_at>start)return {coverage:"unavailable",rows:[]};
  const seen=new Set<string>();
  for(const seat of seats.users){
    if(!seat.userId||seen.has(seat.userId)||!seat.intervals.length)return {coverage:"unavailable",rows:[]};
    seen.add(seat.userId);
    if(seat.intervals.some(([from,until])=>!Number.isSafeInteger(from)||!Number.isSafeInteger(until)||from<start||until>end||from>=until)
      ||JSON.stringify(seat.intervals)!==JSON.stringify(canonicalSeatIntervals(seat.intervals)))
      return {coverage:"unavailable",rows:[]};
  }
  if(seats.users.length===0)return {coverage:"measured",rows:[]};
  const userPlaceholders=seats.users.map(()=>"?").join(",");
  const accounts=(await control.prepare(`SELECT u.id user_id,p.status,
    EXISTS(SELECT 1 FROM platform_assignments x WHERE x.user_id=u.id) platform_identity
    FROM "user" u LEFT JOIN principals p ON p.user_id=u.id
    WHERE u.id IN (${userPlaceholders})`)
    .bind(...seats.users.map(seat=>seat.userId))
    .all<{user_id:string;status:string|null;platform_identity:boolean}>()).results;
  if(accounts.some(account=>account.status===null))return {coverage:"unavailable",rows:[]};
  const billableIds=new Set(accounts.filter(account=>!account.platform_identity).map(account=>account.user_id));
  if(billableIds.size===0)return {coverage:"measured",rows:[]};
  // Control already owns principal identities. No raw user identifier enters a
  // billing meter row; only a Holding/month-domain-separated digest is saved.
  const billablePlaceholders=[...billableIds].map(()=>"?").join(",");
  const events=(await control.prepare(`SELECT id,user_id,status,effective_at
    FROM billing_principal_status_events WHERE effective_at<? AND user_id IN (${billablePlaceholders})
    ORDER BY user_id,effective_at,id`)
    .bind(end,...billableIds)
    .all<{id:number;user_id:string;status:string;effective_at:number}>()).results;
  const byUser=new Map<string,typeof events>();
  for(const event of events){const list=byUser.get(event.user_id)??[];list.push(event);byUser.set(event.user_id,list);}
  const rows:{userKey:string;intervals:SeatInterval[]}[]=[];
  for(const seat of seats.users){
    if(!billableIds.has(seat.userId))continue;
    const timeline=byUser.get(seat.userId)??[];
    if(!timeline.length)return {coverage:"unavailable",rows:[]};
    const active:SeatInterval[]=[];
    for(let i=0;i<timeline.length;i++){
      const event=timeline[i],next=timeline[i+1];
      if(event.status!=="active")continue;
      const from=Math.max(start,event.effective_at),until=Math.min(end,next?.effective_at??end);
      if(from<until)active.push([from,until]);
    }
    const intervals:SeatInterval[]=[];
    for(const source of seat.intervals)for(const principal of active){
      const from=Math.max(source[0],principal[0]),until=Math.min(source[1],principal[1]);
      if(from<until)intervals.push([from,until]);
    }
    const canonical=canonicalSeatIntervals(intervals);
    if(canonical.length)rows.push({userKey:seatKey(holdingId,month,seat.userId),intervals:canonical});
  }
  return {coverage:"measured",rows};
}

function meterNumber(value: number | undefined, field: string): number {
  if (value === undefined) return 0;
  if (!Number.isSafeInteger(value) || value < 0) throw new Error(`INVALID_USAGE_METER:${field}`);
  return value;
}

/**
 * Publish only billing dimensions and numeric meters to Control. A Holding's
 * business records, provider models, prompts, attachment IDs and filenames
 * never enter this table. The digest is stable across retry and refresh.
 */
export async function syncHoldingUsageProjection(
  control: SqlDatabase,
  binding: Binding,
  month: string,
  snapshot: Snapshot,
  observedAt = Date.now(),
): Promise<void> {
  if (!isTransactionalSqlDatabase(control)) throw new Error("USAGE_PROJECTION_TRANSACTION_REQUIRED");
  if (snapshot.holding.holdingId !== binding.id || !/^\d{4}-(0[1-9]|1[0-2])$/.test(month))
    throw new Error("INVALID_USAGE_PROJECTION_SCOPE");
  const meters = new Map<string, MeterRow>();
  for (const item of snapshot.items) {
    if (item.holdingId !== binding.id || item.period !== month || !item.companyId || !item.id)
      throw new Error("INVALID_USAGE_PROJECTION_ITEM");
    const sourceKey = meterKey(binding.id, month, item);
    const funding = item.fundingSource ?? "unknown";
    const status = item.pricingStatus;
    if (!["erpx", "byok", "simulation", "unknown"].includes(funding) ||
        !["byok", "simulation", "unpriced"].includes(status))
      throw new Error("INVALID_USAGE_PROJECTION_STATUS");
    const row: MeterRow = {
      holding_id: binding.id, month, source_key: sourceKey,
      company_id: item.companyId, company_name: item.companyName,
      branch_id: item.branchId, branch_name: item.branchId ? item.branchName : null,
      meter_kind: item.type, funding_source: funding, pricing_status: status,
      quantity: meterNumber(item.quantity, "quantity"),
      input_tokens: meterNumber(item.inputTokens, "input_tokens"),
      output_tokens: meterNumber(item.outputTokens, "output_tokens"),
      average_daily_bytes: meterNumber(item.averageDailyBytes, "average_daily_bytes"),
      active_at_period_end: item.activeAtPeriodEnd === true,
      request_status: item.type === "ai" ? item.requestStatus ?? null : null,
      gateway_status: item.type === "ai" ? item.gatewayStatus ?? null : null,
      provider: item.type === "ai" ? item.provider ?? null : null,
      model: null,
      units_json: item.type === "ai" ? item.unitsJson ?? null : null,
      cost_status: item.costStatus ?? "unavailable",
      verified_cost_satang: item.verifiedCostSatang ?? null,
      cost_evidence_key: item.costEvidenceKey ?? null,
      cost_reconciled_at: item.costReconciledAt ?? null,
      active_from: item.activeFrom ?? null, active_until: item.activeUntil ?? null,
      byte_milliseconds: item.byteMilliseconds ?? null,
    };
    const prior = meters.get(sourceKey);
    if (prior && JSON.stringify(prior) !== JSON.stringify(row))
      throw new Error("CONFLICTING_USAGE_SOURCE");
    meters.set(sourceKey, row);
  }
  const snapshotId = randomUUID();
  const reportedCoverage = (snapshot.holding as UsageHolding & { meterCoverage?: Coverage }).meterCoverage;
  const coverage: Coverage = reportedCoverage ?? { ai: "partial", storage: "partial", seat:"unavailable", unreconciledOriginals: 0 };
  const activeNow=snapshot.holding.seatUsage?.activeNow;
  const activeSeat=activeNow?.status==="measured"&&month===billingMonth(activeNow.asOf)
    ? activeNow : null;
  meterNumber(coverage.unreconciledOriginals, "unreconciled_originals");
  await control.transaction(async tx => {
    await tx.prepare(`INSERT INTO billing_usage_snapshots(holding_id,month,snapshot_id,refreshed_at,source_count,
      ai_coverage,storage_coverage,seat_coverage,seat_source_count,unreconciled_originals,
      active_seat_count,active_seat_observed_at)
      VALUES(?,?,?,?,0,?,?,?,0,?,?,?) ON CONFLICT(holding_id,month) DO NOTHING`)
      .bind(binding.id, month, snapshotId, observedAt, coverage.ai, coverage.storage,"unavailable",
        coverage.unreconciledOriginals,activeSeat?.count??null,activeSeat?.asOf??null).run();
    const current = await tx.prepare(`SELECT refreshed_at FROM billing_usage_snapshots
      WHERE holding_id=? AND month=? FOR UPDATE`).bind(binding.id, month).first<{refreshed_at:number}>();
    if (!current || current.refreshed_at > observedAt) return;
    const seatProjection=await principalSeatIntervals(tx,binding.id,month,snapshot.seats??{coverage:"unavailable",users:[]});
    for (const row of meters.values()) {
      await tx.prepare(`INSERT INTO billing_usage_meters(
        holding_id,month,source_key,company_id,company_name,branch_id,branch_name,meter_kind,
        funding_source,pricing_status,quantity,input_tokens,output_tokens,average_daily_bytes,
        active_at_period_end,snapshot_id,request_status,gateway_status,provider,model,
        units_json,cost_status,verified_cost_satang,cost_evidence_key,cost_reconciled_at,
        active_from,active_until,byte_milliseconds
      ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?::jsonb,?,?,?,?,?,?,?)
      ON CONFLICT(holding_id,month,source_key) DO UPDATE SET
        company_id=excluded.company_id, company_name=excluded.company_name,
        branch_id=excluded.branch_id, branch_name=excluded.branch_name,
        meter_kind=excluded.meter_kind, funding_source=excluded.funding_source,
        pricing_status=excluded.pricing_status, quantity=excluded.quantity,
        input_tokens=excluded.input_tokens, output_tokens=excluded.output_tokens,
        average_daily_bytes=excluded.average_daily_bytes,
        active_at_period_end=excluded.active_at_period_end, snapshot_id=excluded.snapshot_id,
        request_status=excluded.request_status,gateway_status=excluded.gateway_status,
        provider=excluded.provider,model=excluded.model,units_json=excluded.units_json,
        cost_status=excluded.cost_status,verified_cost_satang=excluded.verified_cost_satang,
        cost_evidence_key=excluded.cost_evidence_key,cost_reconciled_at=excluded.cost_reconciled_at,
        active_from=excluded.active_from,active_until=excluded.active_until,
        byte_milliseconds=excluded.byte_milliseconds`)
        .bind(row.holding_id,row.month,row.source_key,row.company_id,row.company_name,
          row.branch_id,row.branch_name,row.meter_kind,row.funding_source,row.pricing_status,
          row.quantity,row.input_tokens,row.output_tokens,row.average_daily_bytes,
          row.active_at_period_end,snapshotId,row.request_status,row.gateway_status,row.provider,
          row.model,row.units_json,row.cost_status,row.verified_cost_satang,
          row.cost_evidence_key,row.cost_reconciled_at,row.active_from,row.active_until,
          row.byte_milliseconds).run();
    }
    await tx.prepare(`DELETE FROM billing_usage_meters
      WHERE holding_id=? AND month=? AND snapshot_id<>?`)
      .bind(binding.id, month, snapshotId).run();
    for(const seat of seatProjection.rows){
      await tx.prepare(`INSERT INTO billing_seat_usage(holding_id,month,user_key,intervals_json,snapshot_id)
        VALUES(?,?,?,?::jsonb,?) ON CONFLICT(holding_id,month,user_key) DO UPDATE SET
        intervals_json=excluded.intervals_json,snapshot_id=excluded.snapshot_id`)
        .bind(binding.id,month,seat.userKey,JSON.stringify(seat.intervals),snapshotId).run();
    }
    await tx.prepare("DELETE FROM billing_seat_usage WHERE holding_id=? AND month=? AND snapshot_id<>?")
      .bind(binding.id,month,snapshotId).run();
    await tx.prepare(`UPDATE billing_usage_snapshots
      SET snapshot_id=?,refreshed_at=?,source_count=?,ai_coverage=?,storage_coverage=?,seat_coverage=?,seat_source_count=?,unreconciled_originals=?,
        active_seat_count=?,active_seat_observed_at=?
      WHERE holding_id=? AND month=?`)
      .bind(snapshotId, observedAt, meters.size, coverage.ai, coverage.storage,
        seatProjection.coverage,seatProjection.rows.length,
        coverage.unreconciledOriginals,activeSeat?.count??null,activeSeat?.asOf??null,
        binding.id, month).run();
  });
}

function emptyMetrics(): UsageMetrics {
  return { ai: { requests: 0, inputTokens: 0, outputTokens: 0 },
    storage: { bytes: 0, fileCount: 0, averageDailyBytes: 0 } };
}

function addMeter(metrics: UsageMetrics, row: MeterRow) {
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

/** Platform queries Control only. Missing projections are explicit, never zero. */
export async function readPlatformUsageProjection(
  control: SqlDatabase,
  bindings: Binding[],
  month: string,
  selectedId: string | null = null,
): Promise<{
  holdings: UsageHolding[];
  items: UsageItem[];
  unavailable: { holdingId: string; holdingName: string; reason: string }[];
}> {
  const selected = selectedId ? bindings.filter(binding => binding.id === selectedId) : bindings;
  const [snapshots, sourceRows, currentSnapshots] = await Promise.all([
    control.prepare(`SELECT holding_id,ai_coverage,storage_coverage,seat_coverage,seat_source_count,unreconciled_originals,
      active_seat_count,active_seat_observed_at
      FROM billing_usage_snapshots WHERE month=?`)
      .bind(month).all<SnapshotHeader>(),
    control.prepare(`SELECT holding_id,month,source_key,company_id,company_name,branch_id,branch_name,
      meter_kind,funding_source,pricing_status,quantity,input_tokens,output_tokens,
      average_daily_bytes,active_at_period_end,request_status,gateway_status,provider,model,
      units_json::text units_json,cost_status,verified_cost_satang,cost_evidence_key,
      cost_reconciled_at,active_from,active_until,byte_milliseconds::text byte_milliseconds
      FROM billing_usage_meters WHERE month=?`)
      .bind(month).all<MeterRow>(),
    control.prepare(`SELECT holding_id,active_seat_count,active_seat_observed_at
      FROM billing_usage_snapshots WHERE month=?`)
      .bind(billingMonth(Date.now())).all<Pick<SnapshotHeader,"holding_id"|"active_seat_count"|"active_seat_observed_at">>(),
  ]);
  const headers = new Map(snapshots.results.map(row => [row.holding_id, row]));
  const activeHeaders=new Map(currentSnapshots.results.map(row=>[row.holding_id,row]));
  const selectedIds = new Set(selected.map(binding => binding.id));
  const rows = sourceRows.results.filter(row => selectedIds.has(row.holding_id));
  const holdings: UsageHolding[] = [];
  const unavailable: { holdingId: string; holdingName: string; reason: string }[] = [];
  const grouped = new Map<string, UsageItem>();
  for (const binding of selected) {
    const header = headers.get(binding.id);
    if (!header) {
      unavailable.push({ holdingId: binding.id, holdingName: binding.displayName, reason: "Meter projection pending" });
      continue;
    }
    const holding: UsageHolding & { meterCoverage: Coverage } = {
      holdingId: binding.id, holdingSlug: binding.slug, holdingName: binding.displayName,
      companies: [], totals: emptyMetrics(),
      sandboxStoragePreview: { status: "unpriced", amountSatang: null },
      meterCoverage: { ai: header.ai_coverage, storage: header.storage_coverage,
        seat: header.seat_coverage, unreconciledOriginals: header.unreconciled_originals },
      seatUsage: {
        monthly: await readControlSeatSummary(control,binding.id,month),
        activeNow: (()=>{const active=activeHeaders.get(binding.id);
          return active?.active_seat_count!==null&&active?.active_seat_count!==undefined&&
            active.active_seat_observed_at!==null&&active.active_seat_observed_at!==undefined
            ? {status:"measured" as const,count:active.active_seat_count,asOf:active.active_seat_observed_at}
            : {status:"unavailable" as const,count:null,asOf:null};})(),
      },
    };
    const companies = new Map<string, UsageCompany>();
    for (const row of rows.filter(value => value.holding_id === binding.id)) {
      let company = companies.get(row.company_id);
      if (!company) {
        company = { companyId: row.company_id, companyName: row.company_name,
          branches: [], unassigned: emptyMetrics(), totals: emptyMetrics() };
        companies.set(row.company_id, company);
        holding.companies.push(company);
      }
      addMeter(holding.totals, row);
      addMeter(company.totals, row);
      if (row.branch_id) {
        let branch = company.branches.find(value => value.branchId === row.branch_id);
        if (!branch) {
          branch = { branchId: row.branch_id, branchName: row.branch_name ?? "Unknown branch", totals: emptyMetrics() };
          company.branches.push(branch);
        }
        addMeter(branch.totals, row);
      } else addMeter(company.unassigned, row);
      // AI attempts stay separate: one business usage row can contain several
      // provider calls (for example a multi-page expense extraction), each with
      // its own cost evidence and reconciliation state.
      const groupKey = row.meter_kind === "ai" ? row.source_key
        : [binding.id,row.company_id,row.branch_id ?? "",row.meter_kind,row.funding_source,row.pricing_status].join(":");
      const prior = grouped.get(groupKey);
      const measuredQuantity = row.meter_kind === "storage" ? row.average_daily_bytes : row.quantity;
      if (prior) {
        prior.quantity += measuredQuantity;
        prior.inputTokens = (prior.inputTokens ?? 0) + row.input_tokens;
        prior.outputTokens = (prior.outputTokens ?? 0) + row.output_tokens;
        prior.averageDailyBytes = (prior.averageDailyBytes ?? 0) + row.average_daily_bytes;
      } else grouped.set(groupKey, {
        id: groupKey, holdingId: binding.id, companyId: row.company_id,
        companyName: row.company_name, branchId: row.branch_id, branchName: row.branch_name,
        type: row.meter_kind, quantity: measuredQuantity,
        unit: row.meter_kind === "ai" ? "tokens" : "bytes", period: month,
        source: row.meter_kind === "ai"
          ? `AI provider · ${row.provider ?? "Unknown provider"} · ${row.request_status ?? "Pending"}`
          : "Original files · monthly daily average",
        pricingStatus: row.pricing_status, estimatedSatang: null, fundingSource: row.funding_source,
        inputTokens: row.input_tokens, outputTokens: row.output_tokens,
        averageDailyBytes: row.average_daily_bytes,
        requestStatus: row.request_status ?? undefined,
        gatewayStatus: row.gateway_status,
        provider: row.provider,
        unitsJson: row.units_json,
        costStatus: row.cost_status,
        verifiedCostSatang: row.verified_cost_satang,
        costEvidenceKey: row.cost_evidence_key,
        costReconciledAt: row.cost_reconciled_at,
      });
    }
    const pricing = sandboxPricingForMonth(month);
    if (pricing && header.storage_coverage === "measured" && header.unreconciled_originals === 0) {
      const preview = previewSandboxStorage({ averageDailyOriginalBytes: holding.totals.storage.averageDailyBytes }, pricing);
      holding.sandboxStoragePreview = { status: preview.status === "estimated" ? "estimated" : "unpriced", amountSatang: preview.amountSatang };
    }
    holding.companies.sort((a,b) => a.companyName.localeCompare(b.companyName));
    for (const company of holding.companies) company.branches.sort((a,b) => a.branchName.localeCompare(b.branchName));
    holdings.push(holding);
  }
  return { holdings, items: [...grouped.values()], unavailable };
}

/** Keep the current and preceding month fresh so late meter rows are included. */
export async function refreshRecentUsageProjections(
  control: SqlDatabase,
  bindings: Binding[],
  now = Date.now(),
): Promise<{ refreshed: number; unavailable: string[] }> {
  const current = billingMonth(now);
  const previous = billingMonth(monthBounds(current).start - 1);
  let refreshed = 0;
  const unavailable: string[] = [];
  for (const binding of bindings) {
    if (binding.id === "h003") continue;
    for (const month of [previous, current]) {
      const observedAt = Date.now();
      try {
        const snapshot = await readHoldingUsage(binding, month);
        await syncHoldingUsageProjection(control, binding, month, snapshot, observedAt);
        refreshed += 1;
      } catch {
        unavailable.push(`${binding.id}:${month}`);
      }
    }
  }
  return { refreshed, unavailable };
}

function shiftMonth(month: string, delta: number): string {
  const [year, number] = month.split("-").map(Number);
  return new Date(Date.UTC(year, number - 1 + delta, 1)).toISOString().slice(0, 7);
}

/**
 * First-run backfill for historical months that have a source meter. The
 * 24-month cap prevents a malformed legacy timestamp from making an unbounded
 * scan; older months remain explicitly unavailable until separately reviewed.
 */
export async function bootstrapUsageProjectionHistory(
  control: SqlDatabase,
  bindings: Binding[],
  now = Date.now(),
  lookbackMonths = 24,
): Promise<{ refreshed: number; unavailable: string[] }> {
  if (!Number.isInteger(lookbackMonths) || lookbackMonths < 1 || lookbackMonths > 24)
    throw new Error("INVALID_USAGE_BACKFILL_WINDOW");
  const current = billingMonth(now);
  const oldestAllowed = shiftMonth(current, 1 - lookbackMonths);
  const recentStart = shiftMonth(current, -1);
  let refreshed = 0;
  const unavailable: string[] = [];
  for (const binding of bindings) {
    if (binding.id === "h003") continue;
    try {
      const source = (await holdingResources(binding.id)).write;
      const [ai, storage, existing] = await Promise.all([
        source.prepare("SELECT MIN(created_at) earliest FROM ai_provider_usage")
          .first<{ earliest: number | null }>(),
        source.prepare("SELECT MIN(created_at) earliest FROM attachment_storage_ledger WHERE variant='original'")
          .first<{ earliest: number | null }>(),
        control.prepare("SELECT month FROM billing_usage_snapshots WHERE holding_id=?")
          .bind(binding.id).all<{ month: string }>(),
      ]);
      const firstTimestamp = [ai?.earliest, storage?.earliest]
        .filter((value): value is number => typeof value === "number")
        .sort((a, b) => a - b)[0];
      if (firstTimestamp === undefined) continue;
      const firstMonth = billingMonth(firstTimestamp);
      const start = firstMonth < oldestAllowed ? oldestAllowed : firstMonth;
      const present = new Set(existing.results.map(row => row.month));
      for (let month = start; month < recentStart; month = shiftMonth(month, 1)) {
        if (present.has(month)) continue;
        try {
          const observedAt = Date.now();
          const snapshot = await readHoldingUsage(binding, month);
          await syncHoldingUsageProjection(control, binding, month, snapshot, observedAt);
          refreshed += 1;
        } catch {
          unavailable.push(`${binding.id}:${month}`);
        }
      }
    } catch {
      unavailable.push(`${binding.id}:history`);
    }
  }
  return { refreshed, unavailable };
}
