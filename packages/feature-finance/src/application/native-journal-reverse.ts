import type { FinanceJournalWorkspaceReadResponse } from '@erpx/contracts/finance-journal-read';
import type { JournalReviewInput, JournalReviewResult } from '@erpx/contracts/finance-journal-post';
import { matchingReadbackContext, type PostContext } from './native-journal-post';
export type ReverseInput = Omit<Extract<JournalReviewInput, { periodId: string }>, 'memo'> & { memo?: string };
export type ReverseReceipt = Extract<JournalReviewResult, { reversalJournalId: string }>;
export type ReverseAttempt = { id: string; version: number; input: ReverseInput; body: string; key: string; context: PostContext; receipt?: ReverseReceipt };
export function reversalReceipt(attempt: ReverseAttempt, value: unknown): ReverseReceipt | undefined {
 if (!value || typeof value !== 'object' || Array.isArray(value)) return;
 const r=value as Record<string,unknown>;
 if (Object.keys(r).sort().join(',') !== 'reversalJournalId,reversedJournalId' || r.reversedJournalId !== attempt.id || r.reversalJournalId !== `reversal-${attempt.key}` || r.reversalJournalId === attempt.id) return;
 return r as ReverseReceipt;
}
export function confirmedReversal(attempt: ReverseAttempt, original: FinanceJournalWorkspaceReadResponse, child: FinanceJournalWorkspaceReadResponse): boolean {
 if (!attempt.receipt || !original?.data?.journal || !child?.data?.journal || !matchingReadbackContext(attempt.context,original) || !matchingReadbackContext(attempt.context,child)) return false;
 const a=original.data.journal,b=child.data.journal;
 return a.id === attempt.id && a.status === 'reversed' && a.version === attempt.version+1 && b.id === attempt.receipt.reversalJournalId && b.status === 'posted' && b.version === 1 && b.source_type === 'reversal' && b.source_id === attempt.id;
}
