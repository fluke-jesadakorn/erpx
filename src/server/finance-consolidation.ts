import "server-only";
import { z } from "zod";
import { hasPermission } from "@/lib/permissions";
import { assertion, executeCommand, replayCommand } from "./commands";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { decimalToMinor, minorToDecimal } from "./finance";

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

type ConsolidationElimination = Record<string, string | number | null> & {
  id: string;
  run_id: string;
  source_transaction_id: string | null;
  debit_account_id: string;
  credit_account_id: string;
  description: string;
  amount_minor: number;
  status: "proposed" | "approved" | "posted";
  version: number;
  approved_by: string | null;
  posted_by: string | null;
  consolidation_journal_id: string | null;
};

type ConsolidationAccount = {
  id: string;
  company_id: string;
  chart_id: string;
  code: string;
  name: string;
  currency: string;
  base_currency: string;
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

export const consolidationEliminationSchema = z.object({
  sourceTransactionId: z.string().trim().min(1).max(160),
  debitAccountId: z.string().trim().min(1).max(160),
  creditAccountId: z.string().trim().min(1).max(160),
  amount: z.string().regex(/^\d+(\.\d{1,6})?$/),
  description: z.string().trim().min(3).max(500),
});

export const consolidationEliminationDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});

export async function consolidationRunWorkspace(actor: WorkContext) {
  actor = holdingActor(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const [runs, eliminations, accounts] = await Promise.all([
    write.prepare("SELECT * FROM finance_consolidation_runs WHERE holding_id=? ORDER BY as_of_date DESC,created_at DESC LIMIT 50").bind(actor.holdingId).all<ConsolidationRun>(),
    write.prepare("SELECT * FROM finance_consolidation_eliminations WHERE run_id IN (SELECT id FROM finance_consolidation_runs WHERE holding_id=?) ORDER BY created_at,id").bind(actor.holdingId).all<ConsolidationElimination>(),
    write.prepare(`SELECT a.id,a.company_id,a.chart_id,a.code,a.name,a.currency,c.base_currency
      FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id
      JOIN finance_group_entities e ON e.company_id=a.company_id AND e.holding_id=? AND e.status='active'
      WHERE a.status='active' AND c.status='active' ORDER BY a.company_id,a.code`).bind(actor.holdingId).all<ConsolidationAccount>(),
  ]);
  const byRun = new Map<string, ConsolidationElimination[]>();
  for (const elimination of eliminations.results) byRun.set(elimination.run_id, [...(byRun.get(elimination.run_id) ?? []), elimination]);
  return {
    runs: runs.results.map(run => ({ ...run, eliminations: byRun.get(run.id) ?? [] })),
    accounts: accounts.results,
    canWrite: hasPermission(actor, "finance.write"),
    canApprove: hasPermission(actor, "finance.approve"),
  };
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

async function consolidationAccounts(write: SqlDatabase, holdingId: string, accountIds: string[], currency: string) {
  const unique = [...new Set(accountIds)];
  const rows = await write.prepare(`SELECT a.id,a.company_id,a.chart_id,a.code,a.name,a.currency,c.base_currency
    FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id
    JOIN finance_group_entities e ON e.company_id=a.company_id AND e.holding_id=? AND e.status='active'
    WHERE a.id IN (${unique.map(() => "?").join(",")}) AND a.status='active' AND c.status='active'`).bind(holdingId, ...unique).all<ConsolidationAccount>();
  if (rows.results.length !== unique.length) throw new ApiError(422, "CONSOLIDATION_ACCOUNT_INVALID", "Choose two active accounts from the Group Finance chart of accounts.");
  if (unique.length !== 2 || unique[0] === unique[1]) throw new ApiError(422, "CONSOLIDATION_ACCOUNT_PAIR_REQUIRED", "Elimination entries require different debit and credit accounts.");
  if (rows.results.some(row => row.currency !== currency || row.base_currency !== currency)) throw new ApiError(422, "CONSOLIDATION_ACCOUNT_CURRENCY", `Elimination accounts must use the ${currency} presentation currency.`);
  if (new Set(rows.results.map(row => row.chart_id)).size !== 1) throw new ApiError(422, "CONSOLIDATION_ACCOUNT_CHART", "Elimination accounts must belong to one Group Finance chart.");
  return new Map(rows.results.map(row => [row.id, row]));
}

async function consolidationRun(write: SqlDatabase, actor: WorkContext, runId: string) {
  const run = await write.prepare("SELECT * FROM finance_consolidation_runs WHERE id=? AND holding_id=?").bind(runId, actor.holdingId).first<ConsolidationRun>();
  if (!run) throw new ApiError(404, "CONSOLIDATION_RUN_NOT_FOUND", "Consolidation run not found in this Holding.");
  return run;
}

export async function createConsolidationElimination(actor: WorkContext, runId: string, input: z.infer<typeof consolidationEliminationSchema>, key: string) {
  actor = holdingActor(actor, "finance.write");
  const { write } = await holdingResources(actor.holdingId);
  const id = `elimination-${key}`;
  const previous = await replayCommand(write, actor, key, "finance.consolidation.elimination.propose", id, { runId, ...input });
  if (previous) return previous;
  const run = await consolidationRun(write, actor, runId);
  if (run.status !== "draft") throw new ApiError(409, "CONSOLIDATION_ELIMINATION_LOCKED", "Add elimination accounts before the consolidation run enters review.");
  const transaction = await write.prepare("SELECT id,status,currency,amount_minor,source_company_id,target_company_id FROM finance_intercompany_transactions WHERE id=? AND holding_id=? AND status IN ('settled','eliminated')").bind(input.sourceTransactionId, actor.holdingId).first<{ id: string; status: string; currency: string; amount_minor: number; source_company_id: string; target_company_id: string }>();
  if (!transaction) throw new ApiError(422, "CONSOLIDATION_SOURCE_REQUIRED", "Choose a settled intercompany transaction as the elimination source.");
  const amount = decimalToMinor(input.amount);
  if (amount <= 0) throw new ApiError(422, "CONSOLIDATION_AMOUNT_INVALID", "The elimination amount must be greater than zero.");
  const accounts = await consolidationAccounts(write, actor.holdingId, [input.debitAccountId, input.creditAccountId], run.presentation_currency);
  const now = Date.now();
  return executeCommand(write, actor, key, "finance.consolidation.elimination.propose", { runId, ...input }, id, 1, { id, runId, status: "proposed", version: 1, amountMinor: amount }, [
    assertion(write, key, "elimination-unique-source", "NOT EXISTS(SELECT 1 FROM finance_consolidation_eliminations WHERE run_id=? AND source_transaction_id=?)", [runId, input.sourceTransactionId]),
    write.prepare(`INSERT INTO finance_consolidation_eliminations(id,run_id,description,debit_account_id,credit_account_id,amount_minor,status,journal_id,version,source_transaction_id,created_by,created_at,updated_at,evidence_json)
      VALUES(?,?,?,?,?,?, 'proposed',NULL,1,?,?,?,?,?)`).bind(id, runId, input.description, input.debitAccountId, input.creditAccountId, amount, input.sourceTransactionId, actor.userId, now, now, JSON.stringify({ sourceTransactionId: transaction.id, sourceStatus: transaction.status, sourceAmountMinor: transaction.amount_minor, debitAccount: accounts.get(input.debitAccountId)?.code, creditAccount: accounts.get(input.creditAccountId)?.code, sourceHash: run.source_hash })),
    write.prepare("INSERT INTO finance_consolidation_elimination_events(id,elimination_id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(`elimination-event-${key}`, id, runId, actor.holdingId, "proposed", null, "proposed", actor.userId, 1, "Elimination account pair prepared from a settled intercompany source.", JSON.stringify({ sourceTransactionId: input.sourceTransactionId, sourceHash: run.source_hash }), now),
  ]);
}

type EliminationAction = "approve" | "post";
const eliminationTransition: Record<EliminationAction, { from: "proposed" | "approved"; to: "approved" | "posted"; permission: string; operation: string; event: "approved" | "posted" }> = {
  approve: { from: "proposed", to: "approved", permission: "finance.approve", operation: "finance.consolidation.elimination.approve", event: "approved" },
  post: { from: "approved", to: "posted", permission: "finance.approve", operation: "finance.consolidation.elimination.post", event: "posted" },
};

export async function decideConsolidationElimination(actor: WorkContext, eliminationId: string, action: EliminationAction, input: z.infer<typeof consolidationEliminationDecisionSchema>, key: string) {
  const rule = eliminationTransition[action];
  actor = holdingActor(actor, rule.permission);
  const { write } = await holdingResources(actor.holdingId);
  const previous = await replayCommand(write, actor, key, rule.operation, eliminationId, input);
  if (previous) return previous;
  const row = await write.prepare(`SELECT e.*,r.status run_status,r.as_of_date,r.presentation_currency,r.source_hash,r.source_snapshot_json
    FROM finance_consolidation_eliminations e JOIN finance_consolidation_runs r ON r.id=e.run_id
    WHERE e.id=? AND r.holding_id=?`).bind(eliminationId, actor.holdingId).first<ConsolidationElimination & { run_status: string; as_of_date: string; presentation_currency: string; source_hash: string; source_snapshot_json: string }>();
  if (!row) throw new ApiError(404, "CONSOLIDATION_ELIMINATION_NOT_FOUND", "Consolidation elimination not found in this Holding.");
  if (row.status !== rule.from || Number(row.version) !== input.expectedVersion) throw new ApiError(409, "CONSOLIDATION_ELIMINATION_CHANGED", `The elimination is no longer ${rule.from}; reload before deciding.`);
  if (action === "approve" && !["review", "approved"].includes(row.run_status)) throw new ApiError(409, "CONSOLIDATION_RUN_NOT_IN_REVIEW", "Review the consolidation snapshot before approving its elimination accounts.");
  if (action === "post" && row.run_status !== "approved") throw new ApiError(409, "CONSOLIDATION_RUN_NOT_APPROVED", "Approve the consolidation snapshot before posting elimination accounts.");
  if (action === "approve" && row.created_by === actor.userId) throw new ApiError(409, "SELF_REVIEW_FORBIDDEN", "A different authorized Finance approver must approve the elimination.");
  if (action === "post" && row.approved_by === actor.userId) throw new ApiError(409, "SELF_REVIEW_FORBIDDEN", "A different authorized Finance approver must post the approved elimination.");
  const storedSource = JSON.parse(String(row.source_snapshot_json)) as { translationRates?: Record<string, string | number> };
  const current = await snapshot(write, actor.holdingId, row.as_of_date, row.presentation_currency, Object.entries(storedSource.translationRates ?? {}).map(([currency, rate]) => ({ currency, rate: minorToDecimal(Number(rate), 6) })));
  if (current.sourceHash !== row.source_hash) throw new ApiError(409, "CONSOLIDATION_SNAPSHOT_STALE", "Closed periods or intercompany evidence changed. Prepare a fresh consolidation snapshot.");
  const accounts = await consolidationAccounts(write, actor.holdingId, [row.debit_account_id, row.credit_account_id], row.presentation_currency);
  const version = input.expectedVersion + 1, now = Date.now();
  const steps: SqlStatement[] = [assertion(write, key, "elimination-version", "EXISTS(SELECT 1 FROM finance_consolidation_eliminations WHERE id=? AND version=? AND status=?)", [eliminationId, input.expectedVersion, rule.from])];
  let result: Record<string, unknown> = { id: eliminationId, status: rule.to, version };
  if (action === "approve") {
    steps.push(
      write.prepare("UPDATE finance_consolidation_eliminations SET status='approved',approved_by=?,approved_at=?,version=version+1,updated_at=? WHERE id=? AND version=? AND status='proposed'").bind(actor.userId, now, now, eliminationId, input.expectedVersion),
    );
  } else {
    const journalId = `consolidation-journal-${row.run_id}`;
    const journalNumber = `CONSOLIDATION-${row.run_id}`;
    const lineCount = await write.prepare("SELECT COUNT(*) count FROM finance_consolidation_journal_lines WHERE journal_id=?").bind(journalId).first<{ count: number }>();
    const ordinal = Number(lineCount?.count ?? 0);
    steps.push(
      assertion(write, key, "elimination-journal-not-reversed", "NOT EXISTS(SELECT 1 FROM finance_consolidation_journals WHERE id=? AND status='reversed')", [journalId]),
      assertion(write, key, "elimination-lines-unique", "NOT EXISTS(SELECT 1 FROM finance_consolidation_journal_lines WHERE id=?)", [`${eliminationId}-debit`]),
      write.prepare(`INSERT INTO finance_consolidation_journals(id,run_id,holding_id,journal_number,entry_date,currency,memo,status,source_hash,version,created_by,posted_by,posted_at,created_at,updated_at)
        VALUES(?,?,?,?,?,?,?,'posted',?,1,?,?,?,?,?) ON CONFLICT(run_id) DO NOTHING`).bind(journalId, row.run_id, actor.holdingId, journalNumber, row.as_of_date, row.presentation_currency, "Controlled Group Finance elimination journal", row.source_hash, row.created_by, actor.userId, now, now, now),
      write.prepare("INSERT INTO finance_consolidation_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,ordinal) VALUES(?,?,?, ?,?,0,?)").bind(`${eliminationId}-debit`, journalId, row.debit_account_id, row.description, row.amount_minor, ordinal),
      write.prepare("INSERT INTO finance_consolidation_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,ordinal) VALUES(?,?,?, ?,0,?,?)").bind(`${eliminationId}-credit`, journalId, row.credit_account_id, row.description, row.amount_minor, ordinal + 1),
      write.prepare("UPDATE finance_consolidation_journals SET status='posted',version=version+1,posted_by=?,posted_at=?,updated_at=? WHERE id=? AND run_id=?").bind(actor.userId, now, now, journalId, row.run_id),
      write.prepare("UPDATE finance_consolidation_eliminations SET status='posted',consolidation_journal_id=?,posted_by=?,posted_at=?,version=version+1,updated_at=? WHERE id=? AND version=? AND status='approved'").bind(journalId, actor.userId, now, now, eliminationId, input.expectedVersion),
    );
    result = { ...result, journalId };
  }
  steps.push(write.prepare("INSERT INTO finance_consolidation_elimination_events(id,elimination_id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(`elimination-event-${key}`, eliminationId, row.run_id, actor.holdingId, rule.event, row.status, rule.to, actor.userId, version, input.reason, JSON.stringify({ sourceHash: row.source_hash, debitAccount: accounts.get(row.debit_account_id)?.code, creditAccount: accounts.get(row.credit_account_id)?.code, amountMinor: row.amount_minor }), now));
  return executeCommand(write, actor, key, rule.operation, input, eliminationId, version, result, steps);
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
  if (action === "approve") {
    const pending = await write.prepare("SELECT 1 FROM finance_consolidation_eliminations WHERE run_id=? AND status='proposed' LIMIT 1").bind(runId).first();
    if (pending) throw new ApiError(409, "CONSOLIDATION_ELIMINATIONS_UNREVIEWED", "Approve every proposed elimination account before approving the consolidation run.");
  }
  if (action === "publish") {
    const pending = await write.prepare("SELECT 1 FROM finance_consolidation_eliminations WHERE run_id=? AND status<>'posted' LIMIT 1").bind(runId).first();
    if (pending) throw new ApiError(409, "CONSOLIDATION_ELIMINATIONS_UNPOSTED", "Post every approved elimination journal before publishing the consolidation run.");
  }
  const storedSource = JSON.parse(String(run.source_snapshot_json)) as { translationRates?: Record<string, string | number> };
  const current = await snapshot(write, actor.holdingId, run.as_of_date, run.presentation_currency, Object.entries(storedSource.translationRates ?? {}).map(([currency, rate]) => ({ currency, rate: minorToDecimal(Number(rate), 6) })));
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

export async function consolidationDetail(actor: WorkContext, id: string) {
 actor = holdingActor(actor, "finance.read");
 const { write } = await holdingResources(actor.holdingId);
 const row = await write.prepare("SELECT * FROM finance_consolidation_runs WHERE id=? AND holding_id=?").bind(id,actor.holdingId).first<ConsolidationRun>();
 if (!row) throw new ApiError(404,"CONSOLIDATION_NOT_FOUND","Consolidation run not found.");
 const data = await consolidationRunWorkspace(actor);
 return { row, data: { ...data, runs: [row], eliminations: (await write.prepare("SELECT * FROM finance_consolidation_eliminations WHERE run_id=? ORDER BY created_at,id").bind(id).all<ConsolidationElimination>()).results } };
}

export async function consolidationEliminationDetail(actor:WorkContext,id:string){
 actor=holdingActor(actor,'finance.read');const {write}=await holdingResources(actor.holdingId);
 const elimination=await write.prepare('SELECT e.* FROM finance_consolidation_eliminations e JOIN finance_consolidation_runs r ON r.id=e.run_id WHERE e.id=? AND r.holding_id=?').bind(id,actor.holdingId).first<ConsolidationElimination>();
 if(!elimination)throw new ApiError(404,'ELIMINATION_NOT_FOUND','Elimination not found.');
 const detail=await consolidationDetail(actor,String(elimination.run_id));
 return {row:elimination,data:{...detail.data,eliminations:[elimination]}};
}
