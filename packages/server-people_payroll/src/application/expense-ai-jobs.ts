import type { StaffActor } from "@erpx/server-operations/public/staff-policy";
import type { AiClaim, EvidenceDocument } from "@erpx/server-people_payroll/public/expense-ai-source";
import type { ExpenseAiResult } from "@erpx/feature-people_payroll/lib/expense-ai";
import { expenseAiDocuments } from "@erpx/server-people_payroll/domain/expense-ai-rules";
import { expenseAiEvidencePages, expenseAiFailure, reconcileExpenseAiResults, validateExpenseAiResult } from "@erpx/server-people_payroll/domain/expense-ai-processing";
import type { ExpenseCommandPort, ExpenseMutation, ExpenseRecord } from "@erpx/server-people_payroll/application/expense-commands";

export type ExpenseAiQueueInput = { paymentId: string | null; expectedVersion: number; retry: boolean; financePolicyVersion?: number };
export type ExpenseAiSourceFacts = { claim: AiClaim; payment: ExpenseRecord | null; evidence: EvidenceDocument[]; sourceVersion: number; digest: string; native: unknown };
export type ExpenseAiConfiguration = { snapshot: string; native: unknown };
export type ExpenseAiFinancePolicy = { version: number; timezone: string; daily_limit: number; monthly_limit: number };
export type ExpenseAiPage = ReturnType<typeof expenseAiEvidencePages>[number];
export interface ExpenseAiQueuePorts extends ExpenseCommandPort<StaffActor> {
  captureActor(actor: StaffActor): Promise<StaffActor>;
  financePolicy(companyId: string, version: number | undefined): Promise<ExpenseAiFinancePolicy | null>;
  enabled(companyId: string): Promise<void>;
  authority(companyId: string): Promise<unknown>;
  source(companyId: string, id: string, paymentId: string | null): Promise<ExpenseAiSourceFacts>;
  allowed(actor: StaffActor, claim: AiClaim, paymentId: string | null): Promise<boolean>;
  configuration(companyId: string): Promise<ExpenseAiConfiguration>;
  existing(actor: StaffActor, id: string, input: ExpenseAiQueueInput, source: ExpenseAiSourceFacts, configuration: ExpenseAiConfiguration): Promise<{ id: string; state: string } | null>;
  now(): number;
  month(now: number): string;
  quota(actor: StaffActor, month: string, now: number): Promise<{ used: number; allowance: number; busy: number } | null>;
  prepareQueue(actor: StaffActor, id: string, input: ExpenseAiQueueInput, key: string, source: ExpenseAiSourceFacts, configuration: ExpenseAiConfiguration, policy: ExpenseAiFinancePolicy | null, authority: unknown, existing: { id: string; state: string } | null, now: number, month: string): Promise<ExpenseMutation>;
}
export function createQueueExpenseAi(ports: ExpenseAiQueuePorts) {
  return async (actor: StaffActor, id: string, input: ExpenseAiQueueInput, key: string) => {
    actor = await ports.captureActor(actor);
    const operation = "expense.ai.prepare", prior = await ports.replay(actor, key, operation, id, input);
    if (prior) return prior;
    const policy = await ports.financePolicy(actor.companyId, input.financePolicyVersion);
    await ports.enabled(actor.companyId);
    const authority = await ports.authority(actor.companyId), source = await ports.source(actor.companyId, id, input.paymentId);
    if (source.sourceVersion !== input.expectedVersion) throw ports.error({ status: 409, code: "EXPENSE_CHANGED", message: "Review the current stage before preparing AI suggestions." });
    if (!await ports.allowed(actor, source.claim, input.paymentId)) throw ports.error({ status: 403, code: "AI_STAGE_FORBIDDEN", message: "Only a person responsible for the current stage can prepare suggestions." });
    const configuration = await ports.configuration(actor.companyId), existing = await ports.existing(actor, id, input, source, configuration);
    if (existing && (!input.retry || !["failed", "stale", "ready"].includes(existing.state))) return { id, proposalId: existing.id, state: existing.state };
    const pages = expenseAiEvidencePages(expenseAiDocuments(source.evidence, input.paymentId), ports.error);
    if (!pages.length) throw ports.error({ status: 422, code: "RECEIPT_REQUIRED", message: "Attach and prepare receipt pages before using AI." });
    const now = ports.now(), month = ports.month(now), quota = await ports.quota(actor, month, now);
    if (quota && quota.used >= quota.allowance) throw ports.error({ status: 429, code: "AI_QUOTA_EXCEEDED", message: "The company AI allowance has been reached. Your draft is safe; complete it manually or review Usage & billing." });
    if (quota && quota.busy >= 2) throw ports.error({ status: 429, code: "AI_BUSY", message: "Two company preparations are already running. Continue editing and retry shortly." });
    return ports.commit(actor, key, operation, input, id, source.claim.version, await ports.prepareQueue(actor, id, input, key, source, configuration, policy, authority, existing, now, month), { companyId: actor.companyId, branchId: source.claim.branch_id, departmentId: source.claim.department_id });
  };
}

export type ExpenseAiJob = { id: string; company_id: string; expense_id: string; payment_draft_id: string | null; stage: string; evidence_digest: string; provider_json: string; actor_json: string | null; finance_policy_version: number | null };
export type ExpenseAiUsage = Record<"image" | "text", { input: number; output: number; attempted: boolean }>;
export type ExpenseAiOutput = { result: unknown; inputTokens: number; outputTokens: number };
export type ExpenseAiContext = { stage: string; claim: AiClaim; payment: ExpenseRecord | null; accounts: { id: string; name: string; currency: string }[]; periods: { id: string }[] };
export interface ExpenseAiWorkerPorts {
  error: ExpenseCommandPort["error"];
  claim(id: string): Promise<{ lease: string; row: ExpenseAiJob } | null>;
  /** The native timer owns renewal I/O; stopping awaits any in-flight renewal. */
  renewal(id: string, lease: string): () => Promise<void>;
  enabled(companyId: string): Promise<void>;
  authorize(actor: StaffActor | null): void;
  financePolicy(companyId: string, version: number | null): Promise<ExpenseAiFinancePolicy | null>;
  authority(companyId: string): Promise<unknown>;
  source(companyId: string, id: string, paymentId: string | null): Promise<ExpenseAiSourceFacts>;
  configuration(companyId: string): Promise<ExpenseAiConfiguration>;
  allowed(actor: StaffActor, claim: AiClaim, paymentId: string | null): Promise<boolean>;
  accounts(companyId: string): Promise<ExpenseAiContext["accounts"]>;
  periods(companyId: string): Promise<ExpenseAiContext["periods"]>;
  renew(id: string, lease: string): Promise<void>;
  imageContent(context: ExpenseAiContext, pages: ExpenseAiPage[]): Promise<unknown>;
  extract(configuration: ExpenseAiConfiguration, row: ExpenseAiJob, actor: StaffActor, lease: string, offset: number, content: unknown): Promise<ExpenseAiOutput>;
  consolidate(configuration: ExpenseAiConfiguration, row: ExpenseAiJob, actor: StaffActor, lease: string, context: ExpenseAiContext, partial: ExpenseAiResult[]): Promise<ExpenseAiOutput>;
  complete(id: string, lease: string, row: ExpenseAiJob, actor: StaffActor, authority: unknown, source: ExpenseAiSourceFacts, result: ExpenseAiResult, inputTokens: number, outputTokens: number, usage: ExpenseAiUsage): Promise<void>;
  errorFacts(error: unknown): { code: string | null; message: string | null; constraint: boolean };
  holdsLease(id: string, lease: string): Promise<boolean>;
  fail(id: string, lease: string, row: ExpenseAiJob, actor: StaffActor | null, stale: boolean, message: string, inputTokens: number, outputTokens: number, usage: ExpenseAiUsage): Promise<void>;
}
export function createRunExpenseAi(ports: ExpenseAiWorkerPorts) {
  return async (id: string) => {
    const claimed = await ports.claim(id);
    if (!claimed) return;
    const { row, lease } = claimed, actor = row.actor_json ? JSON.parse(row.actor_json) as StaffActor : null;
    JSON.parse(row.provider_json); // Reject invalid saved snapshots before starting renewal, as before.
    let inputTokens = 0, outputTokens = 0;
    const usage: ExpenseAiUsage = { image: { input: 0, output: 0, attempted: false }, text: { input: 0, output: 0, attempted: false } };
    const stopRenewal = ports.renewal(id, lease);
    try {
      await ports.enabled(row.company_id);
      ports.authorize(actor);
      await ports.financePolicy(row.company_id, row.finance_policy_version);
      const authority = await ports.authority(row.company_id), source = await ports.source(row.company_id, row.expense_id, row.payment_draft_id), configuration = await ports.configuration(row.company_id);
      if (source.digest !== row.evidence_digest || source.claim.workflow_stage !== row.stage || configuration.snapshot !== row.provider_json) throw ports.error({ status: 409, code: "AI_STALE", message: "The claim, evidence, or AI configuration changed. Prepare new suggestions." });
      if (!actor || !await ports.allowed(actor, source.claim, row.payment_draft_id)) throw ports.error({ status: 409, code: "AI_STALE", message: "The responsible person no longer has access." });
      const pages = expenseAiEvidencePages(expenseAiDocuments(source.evidence, row.payment_draft_id), ports.error), accounts = await ports.accounts(row.company_id), periods = await ports.periods(row.company_id);
      const context: ExpenseAiContext = { stage: row.stage, claim: source.claim, payment: source.payment, accounts, periods }, partial: ExpenseAiResult[] = [];
      const accountCurrencies = new Map(accounts.map(account => [account.id, account.currency])), periodIds = new Set(periods.map(period => period.id));
      for (let offset = 0; offset < pages.length; offset += 2) {
        await ports.renew(id, lease);
        if ((await ports.configuration(row.company_id)).snapshot !== row.provider_json) throw ports.error({ status: 409, code: "AI_STALE", message: "AI configuration changed." });
        const batch = pages.slice(offset, offset + 2), content = await ports.imageContent(context, batch);
        usage.image.attempted = true;
        const output = await ports.extract(configuration, row, actor, lease, offset, content);
        inputTokens += output.inputTokens; outputTokens += output.outputTokens; usage.image.input += output.inputTokens; usage.image.output += output.outputTokens;
        partial.push(validateExpenseAiResult(output.result, batch, row.stage, accountCurrencies, periodIds, String(source.claim.currency ?? "")));
      }
      let result = partial[0];
      if (partial.length > 1) {
        usage.text.attempted = true;
        const output = await ports.consolidate(configuration, row, actor, lease, context, partial);
        inputTokens += output.inputTokens; outputTokens += output.outputTokens; usage.text.input += output.inputTokens; usage.text.output += output.outputTokens;
        result = validateExpenseAiResult(output.result, pages, row.stage, accountCurrencies, periodIds, String(source.claim.currency ?? ""));
      }
      result = reconcileExpenseAiResults(result, partial, row.stage, Number(source.claim.tax_minor ?? 0));
      const refreshed = await ports.source(row.company_id, row.expense_id, row.payment_draft_id);
      if (refreshed.digest !== row.evidence_digest || (await ports.configuration(row.company_id)).snapshot !== row.provider_json || !await ports.allowed(actor, refreshed.claim, row.payment_draft_id)) throw ports.error({ status: 409, code: "AI_STALE", message: "The source or authority changed. Prepare new suggestions." });
      await ports.financePolicy(row.company_id, row.finance_policy_version);
      await ports.complete(id, lease, row, actor, authority, source, result, inputTokens, outputTokens, usage);
    } catch (error) {
      const failure = expenseAiFailure(ports.errorFacts(error));
      if (!await ports.holdsLease(id, lease)) return;
      await ports.fail(id, lease, row, actor, failure.stale, failure.message, inputTokens, outputTokens, usage);
    } finally { await stopRenewal(); }
  };
}

export function createDrainExpenseAi(ports: { queued(expenseId?: string): Promise<string[]>; run(id: string): Promise<void> }) {
  return async (expenseId?: string) => { for (const id of await ports.queued(expenseId)) await ports.run(id); };
}
