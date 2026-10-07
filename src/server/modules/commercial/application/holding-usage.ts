import {previewSandboxStorage,sandboxPricingForMonth} from '@/lib/billing-pricing';
import {resolveUsageMonth,type UsageItem} from '../domain/usage';
import {emptyUsageMetrics,addUsageItem,type UsageCompany,type UsageHolding,type UsageBinding,type UsageNode,type AiRow,type StorageRow} from '../domain/holding-usage';
import type {MonthlySeatSummary,ActiveSeatSnapshot,HoldingSeatMeter} from '../domain/seats';
import type {Clock} from '../../shared';
export interface HoldingUsageRepository{
 facts(binding:UsageBinding,start:number,end:number,branchId:string|null):Promise<{nodes:UsageNode[];ai:AiRow[];storage:StorageRow[];unreconciled:{count:number}|null;reconciliation:{ready:boolean}|null}>;
 seats(month:string):Promise<HoldingSeatMeter>;monthlySeats(binding:UsageBinding,month:string):Promise<MonthlySeatSummary>;activeSeats():Promise<ActiveSeatSnapshot>;
}
export function createHoldingUsageApplication(ports:{clock:Clock;hash(value:string):string;repository(holdingId:string):Promise<HoldingUsageRepository>}){return async(binding:UsageBinding,month:string,branchId:string|null=null):Promise<{holding:UsageHolding;items:UsageItem[];seats:HoldingSeatMeter}>=>{
 const {start,end}=resolveUsageMonth(month,ports.clock.now()),duration=end-start,r=await ports.repository(binding.id),noSeatSummary={status:'unavailable' as const,distinctUsers:null,activeSeatMilliseconds:null},noActiveSnapshot={status:'unavailable' as const,count:null,asOf:null};
 const [facts,seats,monthlySeats,activeNow]=await Promise.all([r.facts(binding,start,end,branchId),branchId===null?r.seats(month):Promise.resolve({coverage:'unavailable' as const,users:[]}),branchId===null?r.monthlySeats(binding,month):Promise.resolve(noSeatSummary),branchId===null?r.activeSeats():Promise.resolve(noActiveSnapshot)]),{nodes,ai,storage,unreconciled,reconciliation}=facts;
 const names = new Map(nodes.map(node => [node.id, node]));
  const companyMap = new Map<string, UsageCompany>();
  const holding: UsageHolding = {
    holdingId: binding.id, holdingSlug: binding.slug, holdingName: binding.displayName,
    companies: [], totals: emptyUsageMetrics(), sandboxStoragePreview: { status: "unpriced", amountSatang: null },
    meterCoverage: { ai: ai.every(row => row.gateway_status === "failed" ||
      (row.gateway_status === "succeeded" && ["succeeded","failed"].includes(row.status ?? "")))
      ? "measured" : "partial",
      storage: unreconciled?.count === 0 && reconciliation?.ready ? "measured" : "partial",
      seat: monthlySeats.status, unreconciledOriginals: unreconciled?.count ?? 0 },
    seatUsage: {monthly:monthlySeats,activeNow},
  };
  const items: UsageItem[] = [];
  function company(companyId: string) {
    let value = companyMap.get(companyId);
    if (!value) {
      value = { companyId, companyName: names.get(companyId)?.display_name ?? "Unknown company", branches: [], unassigned: emptyUsageMetrics(), totals: emptyUsageMetrics() };
      companyMap.set(companyId, value);
      holding.companies.push(value);
    }
    return value;
  }
  function push(item: UsageItem) {
    const target = company(item.companyId);
    addUsageItem(target.totals, item);
    addUsageItem(holding.totals, item);
    if (item.branchId) {
      let branch = target.branches.find(value => value.branchId === item.branchId);
      if (!branch) {
        branch = { branchId: item.branchId, branchName: item.branchName ?? "Unknown branch", totals: emptyUsageMetrics() };
        target.branches.push(branch);
      }
      addUsageItem(branch.totals, item);
    } else addUsageItem(target.unassigned, item);
    items.push(item);
  }
  for (const row of ai) {
    const funding = row.funding_source;
    const requestStatus = row.gateway_status === "failed" || row.status === "failed"
      ? "failed" : row.gateway_status === "succeeded" && row.status === "succeeded"
        ? "succeeded" : undefined;
    // A provider call can succeed and incur ERPX expense even if the business
    // operation fails afterward. Keep that verified expense visible while the
    // rating engine separately excludes the failed business result from charge.
    const costStatus = funding === "byok" || funding === "simulation" || row.gateway_status === "failed"
      ? "not_applicable"
      : !row.verified_at ? "unavailable" : !row.reconciled_at ? "verified" : "reconciled";
    const costEvidenceKey = row.provider_statement_id
      ? ports.hash(`${binding.id}\0${row.id}\0${row.provider_statement_id}`)
      : null;
    push({
      id: row.id, holdingId: binding.id, companyId: row.company_id,
      companyName: names.get(row.company_id)?.display_name ?? "Unknown company",
      branchId: row.branch_id, branchName: row.branch_id ? names.get(row.branch_id)?.display_name ?? "Unknown branch" : null,
      type: "ai", quantity: row.input_tokens + row.output_tokens, unit: "tokens",
      period: month, source: `${row.feature} · ${row.model} · ${requestStatus === "failed" ? "Failed" : requestStatus === "succeeded" ? "Succeeded" : "Pending"}`,
      pricingStatus: funding === "byok" ? "byok" : funding === "simulation" ? "simulation" : "unpriced",
      estimatedSatang: null, fundingSource: funding,
      inputTokens: row.input_tokens, outputTokens: row.output_tokens, occurredAt: row.created_at,
      requestStatus,
      gatewayStatus: row.gateway_status, provider: row.provider, model: row.gateway_model,
      unitsJson: row.units_json, costStatus, verifiedCostSatang: row.verified_cost_satang,
      costEvidenceKey, costReconciledAt: row.reconciled_at,
    });
  }
  for (const row of storage) {
    const activeFrom = Math.max(start, row.created_at);
    const activeUntil = Math.min(end, row.destroyed_at ?? end);
    const byteMilliseconds = BigInt(row.bytes) * BigInt(Math.max(0, activeUntil - activeFrom));
    const averageDailyBytes = Number((byteMilliseconds + BigInt(Math.floor(duration / 2))) / BigInt(duration));
    push({
    id: row.id, holdingId: binding.id, companyId: row.company_id,
    companyName: names.get(row.company_id)?.display_name ?? "Unknown company",
    branchId: row.branch_id, branchName: row.branch_id ? names.get(row.branch_id)?.display_name ?? "Unknown branch" : null,
    type: "storage", quantity: row.bytes, unit: "bytes", period: month,
    source: "Original file", pricingStatus: "unpriced", estimatedSatang: null,
    occurredAt: row.created_at, averageDailyBytes, activeAtPeriodEnd: row.destroyed_at === null || row.destroyed_at >= end,
    activeFrom, activeUntil, byteMilliseconds: byteMilliseconds.toString(), costStatus: "not_applicable",
  });
  }
  const pricing = sandboxPricingForMonth(month);
  if (pricing && branchId === null && holding.meterCoverage.storage === "measured") {
    const preview = previewSandboxStorage({ averageDailyOriginalBytes: holding.totals.storage.averageDailyBytes }, pricing);
    holding.sandboxStoragePreview = { status: preview.status === "estimated" ? "estimated" : "unpriced", amountSatang: preview.amountSatang };
  }
  holding.companies.sort((a, b) => a.companyName.localeCompare(b.companyName));
  for (const entry of holding.companies) entry.branches.sort((a, b) => a.branchName.localeCompare(b.branchName));
  items.sort((a, b) => (b.occurredAt ?? 0) - (a.occurredAt ?? 0) || a.id.localeCompare(b.id));
  return { holding, items, seats };

};}

