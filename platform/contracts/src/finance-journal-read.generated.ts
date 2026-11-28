// Generated from the Rust Finance journal read contracts. Do not edit by hand.
// Regenerate: bun scripts/docs/generate-finance-journal-read-contracts.ts
export type FinanceFieldJournalFlowAccess = { sourceOnly: boolean, canPost: boolean, purchaseControls: { [key in string]: FinancePurchaseAccess }, };
export type FinanceFieldJournalFlowRuntime = "rust" | "legacy";
export type FinanceFieldJournalFlowResponse = { runtime: FinanceFieldJournalFlowRuntime, data: Record<string, unknown> | null, workspace: FinanceJournalRecordWorkspace | null, access: FinanceFieldJournalFlowAccess | null, context: FinanceRenderedWorkContext, };
export type FinanceFieldJournalFlowInput = { stepKey: string, userId: string, expectedVersion: number, recordVersion: number, reason: string, saveDefault: boolean, };
export type FinanceFieldJournalFlowResult = { id: string, version: number, };
export type FinanceFieldJournalFlowCommandResponse = { data: FinanceFieldJournalFlowResult, };
export type FinancePurchaseAccess = { newWork: boolean, settleExisting: boolean, history: boolean, revision: string, };
export type FinanceInvoiceOptions = { charts: Array<Record<string, unknown>>, accounts: Array<Record<string, unknown>>, periods: Array<Record<string, unknown>>, parties: Array<Record<string, unknown>>, invoices: Array<Record<string, unknown>>, };
export type FinanceServiceInvoiceSource = { releaseId: string, orderId: string, commandKey: string, actorId: string, currency: string, amountUnits: string, usdUnits: string, fxRateId: string | null, usedLatestFallback: boolean, };
export type FinanceServiceInvoiceWorkspace = { invoice: Record<string, unknown>, allocations: Array<Record<string, unknown>>, lines: Array<Record<string, unknown>>, lineHistory: Array<Record<string, unknown>>, thaiDocuments: Array<Record<string, unknown>>, customerReceipts: Array<Record<string, unknown>>, options: FinanceInvoiceOptions, source: FinanceServiceInvoiceSource, postingPreview: Record<string, unknown>, };
export type ServiceNoChargeInput = { expectedVersion: number, contextKey: string, reason: string, };
export type ServiceNoChargeStatus = "no_charge_confirmed";
export type ServiceNoChargeResult = { id: string, status: ServiceNoChargeStatus, version: number, confirmationId: string, };
export type ServiceNoChargeResponse = { data: ServiceNoChargeResult, };
export type ServiceNoChargeControls = { invoiceId: string, version: number, contextKey: string, canConfirm: boolean, confirmed: boolean, };
export type FinanceServiceInvoiceWorkspaceResponse = { data: FinanceServiceInvoiceWorkspace, context: FinanceRenderedWorkContext, sourceOnly: boolean, purchaseControls: { [key in string]: FinancePurchaseAccess }, sourcePurchase: Record<string, unknown>, canPost: boolean, flow: Record<string, unknown>, noCharge: ServiceNoChargeControls | null, };
export type FinanceFieldWorkResponse = { data: Array<Record<string, unknown>>, context: FinanceRenderedWorkContext, };
export type FinanceReadInteger = number;
export type FinanceLedgerValue = string | FinanceReadInteger | null;
export type FinanceJournalDetail = { journal: { [key in string]: FinanceLedgerValue }, lines: Array<{ [key in string]: FinanceLedgerValue }>, history: Array<{ [key in string]: FinanceLedgerValue }>, };
export type FinanceJournalRecordWorkspace = { journal: { [key in string]: FinanceLedgerValue }, lines: Array<{ [key in string]: FinanceLedgerValue }>, history: Array<{ [key in string]: FinanceLedgerValue }>, chart: { [key in string]: FinanceLedgerValue }, accounts: Array<{ [key in string]: FinanceLedgerValue }>, periods: Array<{ [key in string]: FinanceLedgerValue }>, dimensions: Array<{ [key in string]: FinanceLedgerValue }>, dimensionValues: Array<{ [key in string]: FinanceLedgerValue }>, };
export type FinanceRenderedWorkContext = { userId: string, holdingId: string, companyId: string, branchId: string | null, departmentId: string | null, branchSlug: string | null, departmentSlug: string | null, assignmentId: string, assignmentVersion: FinanceReadInteger, revision: FinanceReadInteger, role: string, assignmentScopeKind: string, modules: Array<string>, permissions: Array<string>, expiresAt: FinanceReadInteger, };
export type FinanceJournalReadResponse = { data: FinanceJournalDetail, context: FinanceRenderedWorkContext, };
export type FinanceJournalWorkspaceReadResponse = { data: FinanceJournalRecordWorkspace, context: FinanceRenderedWorkContext, };
