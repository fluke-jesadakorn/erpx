import type { FinanceRenderedWorkContext, FinanceJournalWorkspaceReadResponse } from '@erpx/contracts/finance-journal-read';
import type { JournalReviewInput, JournalReviewResult } from '@erpx/contracts/finance-journal-post';

export type PostContext = Omit<FinanceRenderedWorkContext, 'expiresAt'>;
export type PostInput = Extract<JournalReviewInput, { expectedVersion: number }>;
export type PostReceipt = Extract<JournalReviewResult, { id: string }>;
export type PostAttempt = { id: string; input: PostInput; body: string; key: string; context: PostContext; receipt?: PostReceipt };

export function postedReceipt(attempt: PostAttempt, value: unknown): PostReceipt | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return;
  const result = value as Record<string, unknown>;
  if (Object.keys(result).sort().join(',') !== 'id,status,version' || result.id !== attempt.id || result.status !== 'posted' || result.version !== attempt.input.expectedVersion + 1 || !Number.isSafeInteger(result.version)) return;
  return result as PostReceipt;
}
const sameSet = (a: readonly string[], b: readonly string[]) => a.length === b.length && new Set(a).size === new Set(b).size && a.every(v => b.includes(v));
export function confirmedReadback(attempt: PostAttempt, response: FinanceJournalWorkspaceReadResponse): boolean {
  if (!attempt.receipt || !response?.context || !response.data?.journal) return false;
  const { modules, permissions, ...other } = attempt.context;
  const context = response.context;
  return Object.entries(other).every(([key, value]) => context[key as keyof typeof context] === value)
    && Array.isArray(context.modules) && Array.isArray(context.permissions)
    && sameSet(modules, context.modules) && sameSet(permissions, context.permissions)
    && response.data.journal.id === attempt.id && response.data.journal.status === 'posted' && response.data.journal.version === attempt.receipt.version;
}
