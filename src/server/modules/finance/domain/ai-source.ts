import type {z} from 'zod';
import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
import {financeAiPrepareSchema,financeAiResultSchema,type FinanceAiResult} from './ai-contracts';
import type {FinanceError} from './invoice-line-revision';
export type FinanceAiInput = z.infer<typeof financeAiPrepareSchema>;
export type FinanceAiSource = { id: string; label: string; data: Record<string, unknown> };
export type FinanceAiSnapshot = { version: number; companyRevision: number; hash: string; sources: FinanceAiSource[]; href: string; accounts: Record<string, unknown>[]; bankPaymentIds: string[]; input: FinanceAiInput };

export function financeAiScope(actor: WorkContext, permission:(actor:WorkContext,permission:"finance.read")=>WorkContext,error:FinanceError) {
  actor = permission(actor, "finance.read");
  if (actor.beta || actor.branchId || actor.departmentId || actor.assignmentScope !== "holding") throw error(403, "FINANCE_AI_SCOPE_REQUIRED", "Finance AI requires an authorized Company assignment. Continue in the owning workflow at branch or department scope.");
  return actor;
}

/** Reject invented evidence and unsupported financial mutations before publishing. */
export function validateFinanceAiResult(raw: unknown, snapshot: FinanceAiSnapshot,error:FinanceError): FinanceAiResult {
  const result = financeAiResultSchema.parse(raw);
  for (const citation of result.citations) {
    const source = snapshot.sources.find(row => row.id === citation.sourceId);
    if (!source || !Object.hasOwn(source.data, citation.field)) throw error(422, "AI_OUTPUT_INVALID", "AI referenced evidence outside the supplied source facts.");
  }
  if (result.bankPaymentId && (snapshot.input.kind !== "bank_match" || !snapshot.bankPaymentIds.includes(result.bankPaymentId))) throw error(422, "AI_OUTPUT_INVALID", "AI selected an incompatible bank payment.");
  if (result.accounts) {
    const ids = [result.accounts.debitAccountId, result.accounts.creditAccountId, ...(result.accounts.taxAccountId ? [result.accounts.taxAccountId] : [])];
    if (Number(snapshot.sources[0]?.data.tax_minor ?? 0) > 0 && !result.accounts.taxAccountId) throw error(422, "AI_OUTPUT_INVALID", "A taxed invoice requires a distinct tax posting account.");
    if (snapshot.input.kind !== "invoice_review" || new Set(ids).size !== ids.length || ids.some(id => !snapshot.accounts.some(account => account.id === id))) throw error(422, "AI_OUTPUT_INVALID", "AI suggested unavailable or conflicting posting accounts.");
  }
  return result;
}
