import "server-only";
import { z } from "zod";
import { hasPermission } from "@/lib/permissions";
import { decimalToMinor } from "./finance";
import { assertion, executeCommand, replayCommand } from "./commands";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import type { SqlDatabase, SqlStatement } from "./runtime/types";

type IntercompanyStatus = "unmatched" | "matched" | "settled" | "eliminated";
export const intercompanyTransactionSchema = z.object({
  targetCompanyId: z.string().min(1).max(160),
  reference: z.string().trim().min(2).max(120),
  currency: z.string().length(3).transform(value => value.toUpperCase()),
  amount: z.string().regex(/^\d+(\.\d{1,6})?$/),
  sourceJournalId: z.string().min(1).max(160).optional(),
  targetJournalId: z.string().min(1).max(160).optional(),
});
export const intercompanyDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});
export const groupEntitySchema = z.object({
  companyId: z.string().trim().min(1).max(160),
  reportingName: z.string().trim().min(2).max(160),
  functionalCurrency: z.string().trim().length(3).transform(value => value.toUpperCase()),
});

export type IntercompanyRow = Record<string, string | number | null> & {
  id: string;
  source_company_id: string;
  target_company_id: string;
  reference: string;
  currency: string;
  amount_minor: number;
  status: IntercompanyStatus;
  version: number;
  source_journal_id: string | null;
  target_journal_id: string | null;
  created_by: string;
  created_at: number;
};

export type GroupEntity = {
  company_id: string;
  reporting_name: string;
  functional_currency: string;
  status: string;
  version: number;
  created_by: string;
  updated_at: number;
};

function holdingActor(actor: WorkContext, permission: string) {
  actor = requirePermission(actor, permission);
  if (actor.scope.kind !== "holding") throw new ApiError(403, "HOLDING_FINANCE_REQUIRED", "Intercompany finance requires a Holding-scoped Finance assignment.");
  return actor;
}

async function entities(write: SqlDatabase, actor: WorkContext, targetCompanyId: string) {
  const rows = await write.prepare("SELECT company_id FROM finance_group_entities WHERE holding_id=? AND company_id IN (?,?) AND status='active'").bind(actor.holdingId, actor.companyId, targetCompanyId).all<{ company_id: string }>();
  if (rows.results.length !== 2) throw new ApiError(422, "GROUP_ENTITY_REQUIRED", "Both companies must have active Group Finance entities before recording an intercompany item.");
}

async function getTransaction(write: SqlDatabase, actor: WorkContext, id: string) {
  const row = await write.prepare("SELECT * FROM finance_intercompany_transactions WHERE id=? AND holding_id=?").bind(id, actor.holdingId).first<IntercompanyRow>();
  if (!row) throw new ApiError(404, "INTERCOMPANY_NOT_FOUND", "Intercompany transaction not found in this Holding.");
  return row;
}

async function journalGuard(write: SqlDatabase, id: string | undefined, companyId: string, currency: string) {
  if (!id) return;
  const row = await write.prepare("SELECT id FROM finance_journals WHERE id=? AND company_id=? AND currency=? AND status='posted'").bind(id, companyId, currency).first();
  if (!row) throw new ApiError(422, "INTERCOMPANY_JOURNAL_INVALID", "Source and target journals must be posted in the matching company and currency.");
}

export async function intercompanyWorkspace(actor: WorkContext) {
  actor = holdingActor(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const [transactions, groupEntities, availableCompanies] = await Promise.all([
    write.prepare("SELECT * FROM finance_intercompany_transactions WHERE holding_id=? ORDER BY created_at DESC,id DESC LIMIT 200").bind(actor.holdingId).all<IntercompanyRow>(),
    write.prepare("SELECT company_id,reporting_name,functional_currency,status,version,created_by,updated_at FROM finance_group_entities WHERE holding_id=? AND status='active' ORDER BY reporting_name,company_id").bind(actor.holdingId).all<GroupEntity>(),
    write.prepare("SELECT n.id AS company_id,n.display_name FROM organization_nodes n WHERE n.kind='company' AND n.status='active' AND NOT EXISTS (SELECT 1 FROM finance_group_entities e WHERE e.holding_id=? AND e.company_id=n.id) ORDER BY n.display_name,n.id").bind(actor.holdingId).all<{ company_id: string; display_name: string }>(),
  ]);
  return {
    companyId: actor.companyId,
    transactions: transactions.results,
    entities: groupEntities.results,
    availableCompanies: availableCompanies.results,
    canWrite: hasPermission(actor, "finance.write"),
    canApprove: hasPermission(actor, "finance.approve"),
    canPay: hasPermission(actor, "finance.pay"),
    statusCounts: transactions.results.reduce<Record<string, number>>((result, row) => { result[row.status] = (result[row.status] ?? 0) + 1; return result; }, {}),
  };
}

export async function createGroupEntity(actor: WorkContext, input: z.infer<typeof groupEntitySchema>, key: string) {
  actor = holdingActor(actor, "finance.write");
  const { write } = await holdingResources(actor.holdingId);
  const id = `group-entity-${key}`;
  const previous = await replayCommand(write, actor, key, "finance.group-entity.create", id, input);
  if (previous) return previous;
  const company = await write.prepare("SELECT id FROM organization_nodes WHERE id=? AND kind='company' AND status='active'").bind(input.companyId).first<{ id: string }>();
  if (!company) throw new ApiError(422, "GROUP_COMPANY_REQUIRED", "Choose an active Company organization node in this Holding.");
  const existing = await write.prepare("SELECT id FROM finance_group_entities WHERE holding_id=? AND company_id=?").bind(actor.holdingId, input.companyId).first<{ id: string }>();
  if (existing) throw new ApiError(409, "GROUP_ENTITY_EXISTS", "This company is already registered as a Group Finance entity.");
  const now = Date.now();
  return executeCommand(write, actor, key, "finance.group-entity.create", input, id, 1, { id, companyId: input.companyId, status: "active", version: 1 }, [
    assertion(write, key, "group-entity-unique", "NOT EXISTS(SELECT 1 FROM finance_group_entities WHERE holding_id=? AND company_id=?)", [actor.holdingId, input.companyId]),
    write.prepare("INSERT INTO finance_group_entities(id,holding_id,company_id,reporting_name,functional_currency,status,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'active',1,?,?,?)").bind(id, actor.holdingId, input.companyId, input.reportingName, input.functionalCurrency, actor.userId, now, now),
  ], { companyId: input.companyId, branchId: null, departmentId: null });
}

export async function createIntercompany(actor: WorkContext, input: z.infer<typeof intercompanyTransactionSchema>, key: string) {
  actor = holdingActor(actor, "finance.write");
  const { write } = await holdingResources(actor.holdingId);
  const previous = await replayCommand(write, actor, key, "finance.intercompany.create", `intercompany-${key}`, input);
  if (previous) return previous;
  if (input.targetCompanyId === actor.companyId) throw new ApiError(422, "INTERCOMPANY_SAME_COMPANY", "Choose a different target company.");
  await entities(write, actor, input.targetCompanyId);
  const amount = decimalToMinor(input.amount);
  if (amount <= 0) throw new ApiError(422, "INTERCOMPANY_AMOUNT_INVALID", "Intercompany amount must be greater than zero.");
  await journalGuard(write, input.sourceJournalId, actor.companyId, input.currency);
  await journalGuard(write, input.targetJournalId, input.targetCompanyId, input.currency);
  if ((input.sourceJournalId && !input.targetJournalId) || (!input.sourceJournalId && input.targetJournalId)) throw new ApiError(422, "INTERCOMPANY_JOURNAL_PAIR_REQUIRED", "Provide both posted journals or neither journal.");
  const id = `intercompany-${key}`, now = Date.now(), result = { id, status: "unmatched", version: 1, amountMinor: amount };
  return executeCommand(write, actor, key, "finance.intercompany.create", input, id, 1, result, [
    write.prepare("INSERT INTO finance_intercompany_transactions(id,holding_id,source_company_id,target_company_id,reference,currency,amount_minor,source_journal_id,target_journal_id,status,version,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,'unmatched',1,?,?)").bind(id, actor.holdingId, actor.companyId, input.targetCompanyId, input.reference, input.currency, amount, input.sourceJournalId ?? null, input.targetJournalId ?? null, actor.userId, now),
    write.prepare("INSERT INTO finance_intercompany_events(id,transaction_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(`intercompany-event-${key}`, id, actor.holdingId, "created", null, "unmatched", actor.userId, 1, "Intercompany item prepared for independent matching.", JSON.stringify({ sourceJournalId: input.sourceJournalId ?? null, targetJournalId: input.targetJournalId ?? null }), now),
  ]);
}

type DecisionAction = "match" | "settle" | "eliminate";
const transition: Record<DecisionAction, { from: IntercompanyStatus; to: IntercompanyStatus; permission: string; operation: string; event: string }> = {
  match: { from: "unmatched", to: "matched", permission: "finance.approve", operation: "finance.intercompany.match", event: "matched" },
  settle: { from: "matched", to: "settled", permission: "finance.pay", operation: "finance.intercompany.settle", event: "settled" },
  eliminate: { from: "settled", to: "eliminated", permission: "finance.approve", operation: "finance.intercompany.eliminate", event: "eliminated" },
};

export async function decideIntercompany(actor: WorkContext, id: string, action: DecisionAction, input: z.infer<typeof intercompanyDecisionSchema>, key: string) {
  const rule = transition[action];
  actor = holdingActor(actor, rule.permission);
  const { write } = await holdingResources(actor.holdingId);
  const previous = await replayCommand(write, actor, key, rule.operation, id, input);
  if (previous) return previous;
  const row = await getTransaction(write, actor, id);
  if (row.status !== rule.from || Number(row.version) !== input.expectedVersion) throw new ApiError(409, "INTERCOMPANY_CHANGED", `The intercompany item is no longer ${rule.from}; reload before deciding.`);
  const previousEvent = await write.prepare("SELECT actor_id FROM finance_intercompany_events WHERE transaction_id=? AND action IN ('created','matched','settled') ORDER BY version DESC LIMIT 1").bind(id).first<{ actor_id: string }>();
  if (previousEvent?.actor_id === actor.userId) throw new ApiError(409, "SELF_REVIEW_FORBIDDEN", "A different authorized Finance colleague must perform this handoff.");
  const version = input.expectedVersion + 1, now = Date.now(), result = { id, status: rule.to, version };
  const steps: SqlStatement[] = [
    assertion(write, key, "intercompany-version", "EXISTS(SELECT 1 FROM finance_intercompany_transactions WHERE id=? AND holding_id=? AND status=? AND version=?)", [id, actor.holdingId, rule.from, input.expectedVersion]),
    assertion(write, key, "intercompany-independent", "NOT EXISTS(SELECT 1 FROM finance_intercompany_events WHERE transaction_id=? AND version=(SELECT MAX(version) FROM finance_intercompany_events WHERE transaction_id=?) AND actor_id=?)", [id, id, actor.userId]),
    write.prepare("UPDATE finance_intercompany_transactions SET status=?,version=version+1 WHERE id=? AND holding_id=?").bind(rule.to, id, actor.holdingId),
    write.prepare("INSERT INTO finance_intercompany_events(id,transaction_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(`intercompany-event-${key}`, id, actor.holdingId, rule.event, row.status, rule.to, actor.userId, version, input.reason, JSON.stringify({ previousVersion: input.expectedVersion }), now),
  ];
  return executeCommand(write, actor, key, rule.operation, input, id, version, result, steps);
}
