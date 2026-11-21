// Generated from the Rust Finance manual journal edit contracts. Do not edit by hand.
// Regenerate: bun scripts/docs/generate-finance-journal-edit-contracts.ts
export type ManualJournalEditInput = { chartId: string, periodId: string, entryDate: string, currency: string, memo?: string, lines: Array<JournalLineInput>, expectedVersion: number, reason: string, };
export type JournalLineInput = { accountId: string, description?: string, debit: string, credit: string, dimensions?: { [key in string]: string }, };
export type ManualJournalEditResult = { id: string, status: JournalStatus, version: number, };
export type JournalStatus = "draft" | "posted";
