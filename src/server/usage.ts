import "server-only";

import { createHash } from "node:crypto";
import { billingMonth, monthBounds } from "@/lib/billing";
import { previewSandboxStorage, sandboxPricingForMonth } from "@/lib/billing-pricing";
import { ApiError } from "./http";
import { holdingResources, runtimeEnv } from "./runtime";
import type { SqlDatabase } from "./runtime/types";
import { readActiveHoldingSeatCount, readControlSeatSummary, readHoldingSeatIntervals,
  type ActiveSeatSnapshot, type HoldingSeatMeter, type MonthlySeatSummary } from "./seat-meter";

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

type AiRow = {
  id: string; company_id: string; branch_id: string | null;
  model: string; feature: string; status: string | null; input_tokens: number;
  output_tokens: number; funding_source: "erpx" | "byok" | "simulation" | "unknown";
  created_at: number;
  gateway_status: "reserved" | "succeeded" | "failed" | null;
  provider: string | null; gateway_model: string | null; units_json: string | null;
  provider_statement_id: string | null; verified_cost_satang: number | null;
  verified_at: number | null; reconciled_at: number | null;
};
type StorageRow = {
  id: string; company_id: string; branch_id: string | null;
  bytes: number; created_at: number; destroyed_at: number | null;
};
type Node = { id: string; kind: "company" | "branch"; display_name: string; company_id: string };

export function usageMonth(value: string | null): { month: string; start: number; end: number } {
  const month = value ?? billingMonth(Date.now());
  if (!/^\d{4}-(0[1-9]|1[0-2])$/.test(month) || month > billingMonth(Date.now()))
    throw new ApiError(422, "INVALID_MONTH", "Choose this month or an earlier month in YYYY-MM format.");
  const bounds = monthBounds(month);
  return { month, start: bounds.start, end: Math.min(bounds.end, Date.now() + 1) };
}

function emptyMetrics(): UsageMetrics {
  return { ai: { requests: 0, inputTokens: 0, outputTokens: 0 }, storage: { bytes: 0, fileCount: 0, averageDailyBytes: 0 } };
}
function addItem(metrics: UsageMetrics, item: UsageItem) {
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

/** Reads only meter metadata. No prompts, file names, file contents or business records leave the Holding database. */
export async function readHoldingUsage(
  binding: { id: string; slug: string; displayName: string },
  month: string,
  branchId: string | null = null,
): Promise<{ holding: UsageHolding; items: UsageItem[]; seats: HoldingSeatMeter }> {
  const { start, end } = usageMonth(month);
  const duration = end - start;
  const db = (await holdingResources(binding.id)).write as SqlDatabase;
  const control = (await runtimeEnv()).CONTROL_DB;
  const noSeatSummary = { status: "unavailable" as const, distinctUsers: null,
    activeSeatMilliseconds: null };
  const noActiveSnapshot = { status: "unavailable" as const, count: null, asOf: null };
  const [nodes, ai, storage, unreconciled, reconciliation, seats, monthlySeats, activeNow] = await Promise.all([
    db.prepare("SELECT id,kind,display_name,company_id FROM organization_nodes WHERE kind IN ('company','branch')")
      .all<Node>(),
    db.prepare(`SELECT COALESCE(g.id,a.id) id,COALESCE(g.company_id,a.company_id) company_id,
        CASE WHEN g.id IS NOT NULL THEN g.branch_id ELSE a.branch_id END branch_id,COALESCE(g.model,a.model) model,
        COALESCE(g.feature,a.feature) feature,a.status,
        COALESCE(g.input_tokens,a.input_tokens,0) input_tokens,
        COALESCE(g.output_tokens,a.output_tokens,0) output_tokens,
        CASE WHEN g.id IS NULL THEN COALESCE(a.funding_source,'unknown')
             WHEN a.id IS NULL THEN g.funding_source
             WHEN a.branch_id IS DISTINCT FROM g.branch_id
               OR a.model IS DISTINCT FROM g.model
               OR a.feature IS DISTINCT FROM g.feature THEN 'unknown'
             WHEN a.funding_source=g.funding_source THEN g.funding_source
             ELSE 'unknown' END funding_source,
        COALESCE(g.created_at,a.created_at) created_at,
        g.status gateway_status,g.provider,g.model gateway_model,g.units_json::text units_json,
        e.provider_statement_id,e.thb_satang verified_cost_satang,e.verified_at,e.reconciled_at
      FROM ai_gateway_requests g
      FULL OUTER JOIN ai_provider_usage a ON a.id=g.usage_id AND a.company_id=g.company_id
      LEFT JOIN ai_provider_cost_evidence e ON e.request_id=g.id
      WHERE COALESCE(g.created_at,a.created_at)>=? AND COALESCE(g.created_at,a.created_at)<?
        AND (g.holding_id IS NULL OR g.holding_id=?)
        AND (?::text IS NULL OR (CASE WHEN g.id IS NOT NULL THEN g.branch_id ELSE a.branch_id END)=?)
      ORDER BY COALESCE(g.created_at,a.created_at) DESC,COALESCE(g.id,a.id) DESC`)
      .bind(start,end,binding.id,branchId,branchId).all<AiRow>(),
    db.prepare(`SELECT l.id,l.company_id,s.branch_id,l.bytes,l.created_at,l.destroyed_at
      FROM attachment_storage_ledger l
      JOIN attachments a ON a.id=l.attachment_id
      LEFT JOIN upload_intents u ON u.id=a.upload_id
      LEFT JOIN attachment_sets s ON s.id=u.attachment_set_id AND s.company_id=l.company_id
      WHERE l.variant='original' AND l.state IN ('stored','destroyed')
        AND l.created_at<? AND (l.destroyed_at IS NULL OR l.destroyed_at>?)
        AND (?::text IS NULL OR s.branch_id=?)
      ORDER BY l.created_at DESC,l.id DESC`).bind(end, start, branchId, branchId).all<StorageRow>(),
    db.prepare(`SELECT COUNT(*)::int AS count FROM attachments a
      LEFT JOIN upload_intents u ON u.id=a.upload_id
      LEFT JOIN attachment_sets s ON s.id=u.attachment_set_id AND s.company_id=a.company_id
      WHERE a.source_state<>'destroyed' AND a.created_at<?
        AND NOT EXISTS(SELECT 1 FROM attachment_storage_ledger l WHERE l.attachment_id=a.id AND l.variant='original')
        AND (?::text IS NULL OR s.branch_id=?)`).bind(end, branchId, branchId).first<{count:number}>(),
    db.prepare("SELECT ready FROM attachment_reconciliation_runs ORDER BY created_at DESC LIMIT 1").first<{ready:boolean}>(),
    branchId===null ? readHoldingSeatIntervals(db,month) : Promise.resolve({coverage:"unavailable" as const,users:[]}),
    branchId===null ? readControlSeatSummary(control,binding.id,month) : Promise.resolve(noSeatSummary),
    branchId===null ? readActiveHoldingSeatCount(db,control) : Promise.resolve(noActiveSnapshot),
  ]);
  const names = new Map(nodes.results.map(node => [node.id, node]));
  const companyMap = new Map<string, UsageCompany>();
  const holding: UsageHolding = {
    holdingId: binding.id, holdingSlug: binding.slug, holdingName: binding.displayName,
    companies: [], totals: emptyMetrics(), sandboxStoragePreview: { status: "unpriced", amountSatang: null },
    meterCoverage: { ai: ai.results.every(row => row.gateway_status === "failed" ||
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
      value = { companyId, companyName: names.get(companyId)?.display_name ?? "Unknown company", branches: [], unassigned: emptyMetrics(), totals: emptyMetrics() };
      companyMap.set(companyId, value);
      holding.companies.push(value);
    }
    return value;
  }
  function push(item: UsageItem) {
    const target = company(item.companyId);
    addItem(target.totals, item);
    addItem(holding.totals, item);
    if (item.branchId) {
      let branch = target.branches.find(value => value.branchId === item.branchId);
      if (!branch) {
        branch = { branchId: item.branchId, branchName: item.branchName ?? "Unknown branch", totals: emptyMetrics() };
        target.branches.push(branch);
      }
      addItem(branch.totals, item);
    } else addItem(target.unassigned, item);
    items.push(item);
  }
  for (const row of ai.results) {
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
      ? createHash("sha256")
          .update(`${binding.id}\0${row.id}\0${row.provider_statement_id}`).digest("hex")
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
  for (const row of storage.results) {
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
}

/** Platform receives grouped usage only; source IDs and models stay in the tenant database. */
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
