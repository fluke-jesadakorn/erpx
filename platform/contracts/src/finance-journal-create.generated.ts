// Generated from the Rust Finance manual journal creation contracts. Do not edit by hand.
// Regenerate: bun scripts/docs/generate-finance-journal-create-contracts.ts
export type ManualJournalCreateInput = { chartId: string, periodId: string, entryDate: string, currency: string, fxRate?: string, memo?: string, sourceType?: string, sourceId?: string, isOpening?: boolean, lines: Array<JournalLineInput>, };
export type JournalLineInput = { accountId: string, description?: string, debit: string, credit: string, dimensions?: { [key in string]: string }, };
export type PreparedJournalResult = { id: string, number: string, status: JournalStatus, version: number, debit: string, credit: string, fxRateMicros: number, baseCurrency: string, };
export type JournalStatus = "draft" | "posted";
