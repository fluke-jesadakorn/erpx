import "server-only";
import { z } from "zod";
import { hasPermission } from "@/lib/permissions";
import { assertion, executeCommand, replayCommand } from "./commands";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { decimalToMinor } from "./finance";

type EntityPeriod = {
  companyId: string;
  reportingName: string;
  functionalCurrency: string;
  periodId: string;
  periodName: string;
  endDate: string;
  closeVersion: number;
};

type ConsolidationRun = Record<string, string | number | null> & {
  id: string;
  holding_id: string;
  presentation_currency: string;
  as_of_date: string;
  status: "draft" | "review" | "approved" | "published";
  source_snapshot_json: string;
  source_hash: string;
  version: number;
  created_by: string;
  reviewed_by: string | null;
  approved_by: string | null;
  published_by: string | null;
};

export const consolidationRunSchema = z.object({
  asOfDate: z.string().date(),
  presentationCurrency: z.string().trim().length(3).transform(value => value.toUpperCase()),
  translationRates: z.array(z.object({ currency: z.string().trim().length(3).transform(value => value.toUpperCase()), rate: z.string().regex(/^\d+(\.\d{1,6})?$/) })).max(50).default([]),
});

export const consolidationDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});

export async function consolidationRunWorkspace(actor: WorkContext) {
  actor = holdingActor(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const runs = await write.prepare("SELECT * FROM finance_consolidation_runs WHERE holding_id=? ORDER BY as_of_date DESC,created_at DESC LIMIT 50").bind(actor.holdingId).all<ConsolidationRun>();
  return { runs: runs.results, canWrite: hasPermission(actor, "finance.write"), canApprove: hasPermission(actor, "finance.approve") };
}

function holdingActor(actor: WorkContext, permission: string) {
  actor = requirePermission(actor, permission);
  if (actor.scope.kind !== "holding") throw new ApiError(403, "HOLDING_FINANCE_REQUIRED", "Group consolidation requires a Holding-scoped Finance assignment.");
  return actor;
}

async function digest(value: unknown) {
  return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(JSON.stringify(value))))).map(byte => byte.toString(16).padStart(2, "0")).join("");
}

async function entityPeriods(write: SqlDatabase, holdingId: string, asOfDate: string) {
  const entities = await write.prepare("SELECT company_id,reporting_name,functional_currency FROM finance_group_entities WHERE holding_id=? AND status='active' ORDER BY reporting_name,company_id").bind(holdingId).all<{ company_id: string; reporting_name: string; functional_currency: string }>();
  if (entities.results.length < 2) throw new ApiError(409, "CONSOLIDATION_ENTITIES_REQUIRED", "Register at least two active Group Finance entities before preparing consolidation.");
  const result: EntityPeriod[] = [];
  for (const entity of entities.results) {
    const period = await write.prepare("SELECT id,name,end_date,close_version FROM finance_periods WHERE company_id=? AND state='closed' AND end_date<=? ORDER BY end_date DESC,id DESC LIMIT 1").bind(entity.company_id, asOfDate).first<{ id: string; name: string; end_date: string; close_version: number }>();
    if (!period) throw new ApiError(409, "CONSOLIDATION_PERIODS_REQUIRED", `${entity.reporting_name} must have a closed accounting period on or before ${asOfDate}.`);
    result.push({ companyId: entity.company_id, reportingName: entity.reporting_name, functionalCurrency: entity.functional_currency, periodId: period.id, periodName: period.name, endDate: period.end_date, closeVersion: Number(period.close_version) });
  }
  return result;
}

function translationRate(currency: string, presentationCurrency: string, rates: Map<string, number>) {
  if (currency === presentationCurrency) return 1_000_000;
  const rate = rates.get(currency);
  if (rate === undefined) throw new ApiError(422, "CONSOLIDATION_RATE_REQUIRED", `Provide a presentation-currency rate for ${currency} before preparing this snapshot.`);
  if (rate <= 0) throw new ApiError(422, "CONSOLIDATION_RATE_INVALID", `The presentation-currency rate for ${currency} must be greater than zero.`);
  return rate;
}

function translateMinor(amount: number, rateMicros: number) {
  const product = BigInt(amount) * BigInt(rateMicros), negative = product < BigInt(0), absolute = negative ? -product : product;
  const converted = (absolute + BigInt(500_000)) / BigInt(1_000_000), signed = negative ? -converted : converted;
  if (signed > BigInt(Number.MAX_SAFE_INTEGER) || signed < BigInt(Number.MIN_SAFE_INTEGER)) throw new ApiError(422, "MONEY_OVERFLOW", "The translated consolidation balance exceeds supported precision.");
  return Number(signed);
}

async function snapshot(write: SqlDatabase, holdingId: string, asOfDate: string, presentationCurrency: string, rawRates: Array<{ currency: string; rate: string }>) {
  const rates = new Map<string, number>();
  for (const row of rawRates) {
    if (rates.has(row.currency)) throw new ApiError(422, "DUPLICATE_CONSOLIDATION_RATE", "Provide one translation rate per currency.");
    rates.set(row.currency, decimalToMinor(row.rate, 6));
  }
  const entityPeriodsResult = await entityPeriods(write, holdingId, asOfDate);
  const companies = [] as Array<Record<string, unknown>>;
  for (const entity of entityPeriodsResult) {
    const rateMicros = translationRate(entity.functionalCurrency, presentationCurrency, rates);
    const accounts = await write.prepare(`SELECT a.code,a.name,a.category,a.currency,COALESCE(SUM(l.base_debit_minor-l.base_credit_minor),0) AS balance_minor
      FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id JOIN finance_accounts a ON a.id=l.account_id
      WHERE j.company_id=? AND j.period_id=? AND j.status='posted'
      GROUP BY a.code,a.name,a.category,a.currency ORDER BY a.code`).bind(entity.companyId, entity.periodId).all<Record<string, string | number | null>>();
    companies.push({ ...entity, translationRateMicros: rateMicros, accounts: accounts.results.map(account => ({ ...account, translatedBalanceMinor: translateMinor(Number(account.balance_minor ?? 0), rateMicros) })) });
  }
  const intercompany = await write.prepare("SELECT id,source_company_id,target_company_id,reference,currency,amount_minor,status,version FROM finance_intercompany_transactions WHERE holding_id=? AND status IN ('settled','eliminated') ORDER BY id").bind(holdingId).all<Record<string, string | number | null>>();
  const source = { asOfDate, presentationCurrency, translationRates: Object.fromEntries(rates), companies, intercompany: intercompany.results };
  return { source, sourceHash: await digest(source) };
}

export async function createConsolidationRun(actor: WorkContext, input: z.infer<typeof consolidationRunSchema>, key: string) {
  actor = holdingActor(actor, "finance.write");
  const { write } = await holdingResources(actor.holdingId);
  const id = `consolidation-${key}`;
  const previous = await replayCommand(write, actor, key, "finance.consolidation.prepare", id, input);
  if (previous) return previous;
  const duplicate = await write.prepare("SELECT id FROM finance_consolidation_runs WHERE holding_id=? AND as_of_date=? AND presentation_currency=?").bind(actor.holdingId, input.asOfDate, input.presentationCurrency).first<{ id: string }>();
  if (duplicate) throw new ApiError(409, "CONSOLIDATION_RUN_EXISTS", "A consolidation run already exists for this date and presentation currency.");
  const { source, sourceHash } = await snapshot(write, actor.holdingId, input.asOfDate, input.presentationCurrency, input.translationRates);
  const now = Date.now();
  return executeCommand(write, actor, key, "finance.consolidation.prepare", input, id, 1, { id, status: "draft", version: 1, sourceHash, companyCount: source.companies.length }, [
    assertion(write, key, "consolidation-unique", "NOT EXISTS(SELECT 1 FROM finance_consolidation_runs WHERE holding_id=? AND as_of_date=? AND presentation_currency=?)", [actor.holdingId, input.asOfDate, input.presentationCurrency]),
    write.prepare("INSERT INTO finance_consolidation_runs(id,holding_id,presentation_currency,as_of_date,status,source_snapshot_json,source_hash,version,created_by,created_at,updated_at) VALUES(?,?,?,?, 'draft',?,?,1,?,?,?)").bind(id, actor.holdingId, input.presentationCurrency, input.asOfDate, JSON.stringify(source), sourceHash, actor.userId, now, now),
    write.prepare("INSERT INTO finance_consolidation_events(id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(`consolidation-event-${key}`, id, actor.holdingId, "prepared", null, "draft", actor.userId, 1, "Consolidation snapshot prepared from closed company periods.", JSON.stringify({ sourceHash, companyCount: source.companies.length }), now),
  ]);
}

const transition = {
  review: { from: "draft", to: "review", permission: "finance.write", operation: "finance.consolidation.review", event: "reviewed" },
  approve: { from: "review", to: "approved", permission: "finance.approve", operation: "finance.consolidation.approve", event: "approved" },
  publish: { from: "approved", to: "published", permission: "finance.approve", operation: "finance.consolidation.publish", event: "published" },
} as const;

export async function decideConsolidationRun(actor: WorkContext, runId: string, action: keyof typeof transition, input: z.infer<typeof consolidationDecisionSchema>, key: string) {
  const rule = transition[action];
  actor = holdingActor(actor, rule.permission);
  const { write } = await holdingResources(actor.holdingId);
  const previous = await replayCommand(write, actor, key, rule.operation, runId, input);
  if (previous) return previous;
  const run = await write.prepare("SELECT * FROM finance_consolidation_runs WHERE id=? AND holding_id=?").bind(runId, actor.holdingId).first<ConsolidationRun>();
  if (!run) throw new ApiError(404, "CONSOLIDATION_RUN_NOT_FOUND", "Consolidation run not found in this Holding.");
  if (run.status !== rule.from || Number(run.version) !== input.expectedVersion) throw new ApiError(409, "CONSOLIDATION_CHANGED", `The consolidation run is no longer ${rule.from}; reload before deciding.`);
  if (run.created_by === actor.userId || (action === "approve" && run.reviewed_by === actor.userId) || (action === "publish" && run.approved_by === actor.userId)) throw new ApiError(409, "SELF_REVIEW_FORBIDDEN", "A different authorized Finance colleague must perform this handoff.");
  const storedSource = JSON.parse(String(run.source_snapshot_json)) as { translationRates?: Record<string, string | number> };
  const current = await snapshot(write, actor.holdingId, run.as_of_date, run.presentation_currency, Object.entries(storedSource.translationRates ?? {}).map(([currency, rate]) => ({ currency, rate: String(rate) })));
  if (current.sourceHash !== run.source_hash) throw new ApiError(409, "CONSOLIDATION_SNAPSHOT_STALE", "Closed periods or intercompany evidence changed. Prepare a fresh consolidation snapshot.");
  const version = input.expectedVersion + 1, now = Date.now();
  const updates = action === "review" ? "status=?,reviewed_by=?,reviewed_at=?,version=version+1,updated_at=?" : action === "approve" ? "status=?,approved_by=?,version=version+1,updated_at=?" : "status=?,published_by=?,published_at=?,version=version+1,updated_at=?";
  const values = action === "review" ? [rule.to, actor.userId, now, now] : action === "approve" ? [rule.to, actor.userId, now] : [rule.to, actor.userId, now, now];
  const update = write.prepare(`UPDATE finance_consolidation_runs SET ${updates} WHERE id=? AND holding_id=? AND status=? AND version=?`).bind(...values, runId, actor.holdingId, rule.from, input.expectedVersion);
  const event = write.prepare("INSERT INTO finance_consolidation_events(id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(`consolidation-event-${key}`, runId, actor.holdingId, rule.event, run.status, rule.to, actor.userId, version, input.reason, JSON.stringify({ sourceHash: run.source_hash, previousVersion: input.expectedVersion }), now);
  const steps: SqlStatement[] = [
    assertion(write, key, "consolidation-version", "EXISTS(SELECT 1 FROM finance_consolidation_runs WHERE id=? AND holding_id=? AND status=? AND version=?)", [runId, actor.holdingId, rule.from, input.expectedVersion]),
    update,
    event,
  ];
  return executeCommand(write, actor, key, rule.operation, input, runId, version, { id: runId, status: rule.to, version }, steps);
}
