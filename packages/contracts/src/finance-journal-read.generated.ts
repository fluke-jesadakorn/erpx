// Generated from the Rust Finance journal read contracts. Do not edit by hand.
// Regenerate: bun scripts/docs/generate-finance-journal-read-contracts.ts
export type FinanceInvoiceOptions = { charts: Array<Record<string, unknown>>, accounts: Array<Record<string, unknown>>, periods: Array<Record<string, unknown>>, parties: Array<Record<string, unknown>>, invoices: Array<Record<string, unknown>>, };
export type FinanceServiceInvoiceSource = { releaseId: string, orderId: string, commandKey: string, actorId: string, currency: string, amountUnits: string, usdUnits: string, fxRateId: string | null, usedLatestFallback: boolean, };
export type FinanceServiceInvoiceWorkspace = { invoice: Record<string, unknown>, allocations: Array<Record<string, unknown>>, lines: Array<Record<string, unknown>>, lineHistory: Array<Record<string, unknown>>, thaiDocuments: Array<Record<string, unknown>>, options: FinanceInvoiceOptions, source: FinanceServiceInvoiceSource, };
export type FinanceServiceInvoiceWorkspaceResponse = { data: FinanceServiceInvoiceWorkspace, context: FinanceRenderedWorkContext, sourceOnly: boolean, };
export type FinanceReadInteger = number;
export type FinanceLedgerValue = string | FinanceReadInteger | null;
export type FinanceJournalDetail = { journal: { [key in string]: FinanceLedgerValue }, lines: Array<{ [key in string]: FinanceLedgerValue }>, history: Array<{ [key in string]: FinanceLedgerValue }>, };
export type FinanceJournalRecordWorkspace = { journal: { [key in string]: FinanceLedgerValue }, lines: Array<{ [key in string]: FinanceLedgerValue }>, history: Array<{ [key in string]: FinanceLedgerValue }>, chart: { [key in string]: FinanceLedgerValue }, accounts: Array<{ [key in string]: FinanceLedgerValue }>, periods: Array<{ [key in string]: FinanceLedgerValue }>, dimensions: Array<{ [key in string]: FinanceLedgerValue }>, dimensionValues: Array<{ [key in string]: FinanceLedgerValue }>, };
export type FinanceRenderedWorkContext = { userId: string, holdingId: string, companyId: string, branchId: string | null, departmentId: string | null, branchSlug: string | null, departmentSlug: string | null, assignmentId: string, assignmentVersion: FinanceReadInteger, revision: FinanceReadInteger, role: string, assignmentScopeKind: string, modules: Array<string>, permissions: Array<string>, expiresAt: FinanceReadInteger, };
export type FinanceJournalReadResponse = { data: FinanceJournalDetail, context: FinanceRenderedWorkContext, };
export type FinanceJournalWorkspaceReadResponse = { data: FinanceJournalRecordWorkspace, context: FinanceRenderedWorkContext, };
