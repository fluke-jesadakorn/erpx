// Generated from the Rust Finance journal review contracts. Do not edit by hand.
// Regenerate: bun scripts/docs/generate-finance-journal-post-contracts.ts
export type JournalReviewInput = { expectedVersion: number, } | { periodId: string, entryDate: string, memo: string | null, };
export type JournalReviewResult = { id: string, status: string, version: number, } | { reversalJournalId: string, reversedJournalId: string, };
