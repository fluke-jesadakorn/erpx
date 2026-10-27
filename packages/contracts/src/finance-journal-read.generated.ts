// Generated from the Rust Finance journal read contracts. Do not edit by hand.
// Regenerate: bun scripts/docs/generate-finance-journal-read-contracts.ts
export type FinanceReadInteger = number;
export type FinanceLedgerValue = string | FinanceReadInteger | null;
export type FinanceJournalDetail = { journal: { [key in string]: FinanceLedgerValue }, lines: Array<{ [key in string]: FinanceLedgerValue }>, history: Array<{ [key in string]: FinanceLedgerValue }>, };
export type FinanceRenderedWorkContext = { userId: string, holdingId: string, companyId: string, branchId: string | null, departmentId: string | null, branchSlug: string | null, departmentSlug: string | null, assignmentId: string, assignmentVersion: FinanceReadInteger, revision: FinanceReadInteger, role: string, assignmentScopeKind: string, modules: Array<string>, permissions: Array<string>, expiresAt: FinanceReadInteger, };
export type FinanceJournalReadResponse = { data: FinanceJournalDetail, context: FinanceRenderedWorkContext, };
