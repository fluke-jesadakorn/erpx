import {billingMonth,monthBounds} from '@/lib/billing';
import {previewSandboxStorage,sandboxPricingForMonth} from '@/lib/billing-pricing';
import type {Clock,IdentifierSource,UnitOfWork} from '../../shared';
import type {UsageCompany,UsageHolding} from '../domain/holding-usage';
import type {UsageItem} from '../domain/usage';
import {canonicalSeatIntervals,type HoldingSeatMeter,type SeatInterval,type MonthlySeatSummary} from '../domain/seats';
import {meterNumber,emptyMetrics,addMeter,shiftMonth,type Binding,type Snapshot,type Coverage,type SnapshotHeader,type MeterRow,type PrincipalAccount,type PrincipalEvent,type ProjectedSeats,type ProjectionPlan} from '../domain/projection';
export interface PrincipalSeatRepository{principalCoverage():Promise<{since_at:number}|null>;principalAccounts(ids:string[]):Promise<PrincipalAccount[]>;principalEvents(end:number,ids:string[]):Promise<PrincipalEvent[]>;}
export interface UsageProjectionTransaction extends PrincipalSeatRepository{
 ensureHeader(plan:ProjectionPlan):Promise<void>;lockHeader(holdingId:string,month:string):Promise<{refreshed_at:number}|null>;
 meter(plan:ProjectionPlan,row:MeterRow):Promise<void>;pruneMeters(plan:ProjectionPlan):Promise<void>;seat(plan:ProjectionPlan,seat:ProjectedSeats['rows'][number]):Promise<void>;pruneSeats(plan:ProjectionPlan):Promise<void>;finish(plan:ProjectionPlan,sourceCount:number,seats:ProjectedSeats):Promise<void>;
}
export interface UsageProjectionRepository{
 projectionFacts(month:string,currentMonth:string):Promise<{snapshots:SnapshotHeader[];sourceRows:MeterRow[];currentSnapshots:Pick<SnapshotHeader,'holding_id'|'active_seat_count'|'active_seat_observed_at'>[]}>;
 seatSummary(holdingId:string,month:string):Promise<MonthlySeatSummary>;
 history(binding:Binding):Promise<{ai:{earliest:number|null}|null;storage:{earliest:number|null}|null;existing:{month:string}[]}>;
}
export async function principalSeatIntervals(repository:PrincipalSeatRepository,holdingId:string,month:string,seats:HoldingSeatMeter,hash:(value:string)=>string):Promise<ProjectedSeats>{
  if(seats.coverage!=="measured")return {coverage:"unavailable",rows:[]};
  const {start,end}=monthBounds(month);
  const coverage=await repository.principalCoverage();
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
  const accounts=await repository.principalAccounts(seats.users.map(seat=>seat.userId));
  if(accounts.some(account=>account.status===null))return {coverage:"unavailable",rows:[]};
  const billableIds=new Set(accounts.filter(account=>!account.platform_identity).map(account=>account.user_id));
  if(billableIds.size===0)return {coverage:"measured",rows:[]};
  // Control already owns principal identities. No raw user identifier enters a
  // billing meter row; only a Holding/month-domain-separated digest is saved.
  const events=await repository.principalEvents(end,[...billableIds]);
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
    if(canonical.length)rows.push({userKey:hash(`billing-seat-v1\0${holdingId}\0${month}\0${seat.userId}`),intervals:canonical});
  }
  return {coverage:"measured",rows};
}
export function createUsageProjectionApplication(ports:{repository:UsageProjectionRepository;transactions?:UnitOfWork<UsageProjectionTransaction>;clock:Clock;identifiers:IdentifierSource;hash(value:string):string;readHolding(binding:Binding,month:string):Promise<Snapshot>}){
 async function syncHoldingUsageProjection(binding:Binding,month:string,snapshot:Snapshot,observedAt:number):Promise<void>{if (!ports.transactions) throw new Error("USAGE_PROJECTION_TRANSACTION_REQUIRED");
  if (snapshot.holding.holdingId !== binding.id || !/^\d{4}-(0[1-9]|1[0-2])$/.test(month))
    throw new Error("INVALID_USAGE_PROJECTION_SCOPE");
  const meters = new Map<string, MeterRow>();
  for (const item of snapshot.items) {
    if (item.holdingId !== binding.id || item.period !== month || !item.companyId || !item.id)
      throw new Error("INVALID_USAGE_PROJECTION_ITEM");
    const sourceKey = ports.hash(`${binding.id}\0${month}\0${item.type}\0${item.id}`);
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
  const snapshotId = ports.identifiers.next();
  const reportedCoverage = (snapshot.holding as UsageHolding & { meterCoverage?: Coverage }).meterCoverage;
  const coverage: Coverage = reportedCoverage ?? { ai: "partial", storage: "partial", seat:"unavailable", unreconciledOriginals: 0 };
  const activeNow=snapshot.holding.seatUsage?.activeNow;
  const activeSeat=activeNow?.status==="measured"&&month===billingMonth(activeNow.asOf)
    ? activeNow : null;
  meterNumber(coverage.unreconciledOriginals, "unreconciled_originals");
  const plan:ProjectionPlan={binding,month,snapshotId,observedAt,coverage,activeSeat};
await ports.transactions.run(async transaction=>{
 await transaction.ensureHeader(plan);const current=await transaction.lockHeader(binding.id,month);if(!current||current.refreshed_at>observedAt)return;
 const seatProjection=await principalSeatIntervals(transaction,binding.id,month,snapshot.seats??{coverage:'unavailable',users:[]},ports.hash);
 for(const row of meters.values())await transaction.meter(plan,row);await transaction.pruneMeters(plan);for(const seat of seatProjection.rows)await transaction.seat(plan,seat);await transaction.pruneSeats(plan);await transaction.finish(plan,meters.size,seatProjection);
});}
 async function readPlatformUsageProjection(bindings:Binding[],month:string,selectedId:string|null=null):Promise<{holdings:UsageHolding[];items:UsageItem[];unavailable:{holdingId:string;holdingName:string;reason:string}[]}>{
  const selected = selectedId ? bindings.filter(binding => binding.id === selectedId) : bindings;
  const {snapshots,sourceRows,currentSnapshots}=await ports.repository.projectionFacts(month,billingMonth(ports.clock.now()));
const headers = new Map(snapshots.map(row => [row.holding_id, row]));
  const activeHeaders=new Map(currentSnapshots.map(row=>[row.holding_id,row]));
  const selectedIds = new Set(selected.map(binding => binding.id));
  const rows = sourceRows.filter(row => selectedIds.has(row.holding_id));
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
        monthly: await ports.repository.seatSummary(binding.id,month),
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
 async function refreshRecentUsageProjections(bindings:Binding[],now:number){
  const current = billingMonth(now);
  const previous = billingMonth(monthBounds(current).start - 1);
  let refreshed = 0;
  const unavailable: string[] = [];
  for (const binding of bindings) {
    if (binding.id === "h003") continue;
    for (const month of [previous, current]) {
      const observedAt = ports.clock.now();
      try {
        const snapshot = await ports.readHolding(binding,month);
        await syncHoldingUsageProjection(binding,month,snapshot,observedAt);
        refreshed += 1;
      } catch {
        unavailable.push(`${binding.id}:${month}`);
      }
    }
  }
  return { refreshed, unavailable };
}
 async function bootstrapUsageProjectionHistory(bindings:Binding[],now:number,lookbackMonths:number){
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
      const {ai,storage,existing}=await ports.repository.history(binding);
const firstTimestamp = [ai?.earliest, storage?.earliest]
        .filter((value): value is number => typeof value === "number")
        .sort((a, b) => a - b)[0];
      if (firstTimestamp === undefined) continue;
      const firstMonth = billingMonth(firstTimestamp);
      const start = firstMonth < oldestAllowed ? oldestAllowed : firstMonth;
      const present = new Set(existing.map(row => row.month));
      for (let month = start; month < recentStart; month = shiftMonth(month, 1)) {
        if (present.has(month)) continue;
        try {
          const observedAt = ports.clock.now();
          const snapshot = await ports.readHolding(binding,month);
          await syncHoldingUsageProjection(binding,month,snapshot,observedAt);
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
 return {syncHoldingUsageProjection,readPlatformUsageProjection,refreshRecentUsageProjections,bootstrapUsageProjectionHistory};
}
