import {serviceCustomerSchema} from '../domain/service-customer-preparation';
import {withServiceCustomerCommand} from './service-customer-preparation';
import {withSupplierReversalMoney,prepareSupplierReversalMoney} from './supplier-payment-money';
import {withJournalReversalMoneyUnit,prepareJournalReversalMoney} from './journal-reversal-money';
import {expensePostingChartSchema,expensePostingAccountSchema,expensePostingPeriodSchema,type ExpensePostingChartResult,type ExpensePostingAccountResult,type ExpensePostingPeriodResult} from '../domain/expense-posting-prerequisites';
import {withExpensePostingPrerequisiteCommand} from './expense-posting-prerequisites';
import {payrollPostingChartSchema,payrollPostingAccountSchema,payrollPostingPeriodSchema,type PayrollPostingChartResult,type PayrollPostingAccountResult,type PayrollPostingPeriodResult} from '../domain/payroll-posting-prerequisites';
import {withPayrollPostingPrerequisiteCommand} from './payroll-posting-prerequisites';
import {withPayrollOtherBankEvidence,payrollOtherBankCommandInput,payrollOtherBankStoredResult,publicPayrollOtherBankResult,type PayrollOtherBankWitness} from './payroll-other-bank-evidence';
import type {PayrollOtherBankEvidenceInput} from '../../people_payroll/public/payroll-other-remittance-contract';
import {withManufacturingPostingPrerequisiteCommand} from './manufacturing-posting-prerequisites';
import {manufacturingPostingChartSchema,manufacturingPostingAccountSchema,manufacturingPostingPeriodSchema} from '../domain/manufacturing-posting-prerequisites';
import {refundBankEvidenceSchema} from '../domain/refund-bank-evidence';
import {withRefundBankEvidenceImport,refundBankEvidenceCommandInput,refundBankEvidenceMarker,refundBankEvidenceStoredResult,publicBankImportResult,type RefundBankEvidenceWitness} from './refund-bank-evidence';
import {withSalesReceiptReversal,salesReceiptReversalPurchase} from './sales-payment-reversal';
import {withSupplierPaymentCorrection,supplierPaymentCorrectionPurchase,supplierPaymentCorrectionSourceIds} from "./supplier-payment-corrections";
import {withServicePaymentCommand,servicePaymentPurchase} from "./service-payment-purchases";
import {requireFinanceDocumentHistory,withFinanceDocumentPost} from "./document-purchases";
import { salesInvoiceLines } from "./sales-lines";
import { withFinancePurchasedCommand, requireFinancePurchasedRead, requireFinanceReferenceRead, withFinanceConfigurationCommand } from "./purchase-boundary";
import {fieldServiceFinanceRecordSources} from "../../field_service/public/finance-record-sources";
import type {BankImportInput} from "../application/bank";
import { financePackageEnforcementMode } from "../../commercial/public/purchase-command-boundary";
import { financeTransaction } from "../public/finance-transactions";
import { ensureCustomerReceipt } from "../public/sales";
import { clearWorkflowCommandAssertions } from "../../workflow/public/command-assertion-effects";
import { financeOwnerDecisionEffects } from "../../workflow/public/finance-owner-decision-effects";
export type { FinanceRow } from "../domain/finance";
import {financeCommandReceipts} from "../../workflow/public/finance-command-receipts";
import {financeCloseProcurementSource} from "../../procurement/public/finance-close-source";
import {financeCloseCommerceSource} from "../../commerce/public/finance-close-source";
import {financeCloseManufacturingSource} from "../../manufacturing/public/finance-close-source";
import {createFinancePrior,createFinanceResourcePolicy} from '../application/access';
import { createLegacyRevenue } from '../application/legacy-revenue';
import { createFinanceClose } from '../application/close';
import { createFxRevaluation } from '../application/fx-revaluation';
import { createFinanceBank } from '../application/bank';
import { createFinancePayments } from '../application/payments';
import { createInvoiceService } from '../application/invoices';
import { createJournalBatch } from '../application/journal-batch';
import { createFinanceReads } from '../application/finance-reads';
import { createJournalReview, type JournalPostPlan } from '../application/journal-review';
import { createAccountImport } from '../application/account-import';
import { createOpeningService } from '../application/opening';
import type { JournalPlan } from '../application/journal';
import type { JournalInput } from '../domain/finance';
import { createFinanceConfiguration } from '../application/configuration';
import {withSalesPostingCommand} from './sales-posting-setup';
import {salesPostingChartSchema,salesPostingAccountSchema,salesPostingPeriodSchema} from '../domain/sales-posting-setup';
import {withSupplyPostingPrerequisiteCommand} from './supply-posting-prerequisites';
import {supplyPostingChartSchema,supplyPostingAccountSchema,supplyPostingPeriodSchema} from '../domain/supply-posting-prerequisites';
import { createJournalPreparation } from '../application/journal';
import { createEnsurePeriod } from '../application/period-policy';
import { createFunctionalFx } from '../application/functional-fx';
import { createFinancePolicy } from '../domain/finance-policy';
const { decimalToMinor } = createFinancePolicy((...args) => new ApiError(...args));
export { decimalToMinor };
import { createJournalSchema, openingTrialBalanceSchema, createChartSchema, createAccountSchema, changeAccountStatusSchema, createPeriodSchema, closeProofSchema, reversePaymentSchema, journalBatchSchema, prepareFxRevaluationSchema, decideFxRevaluationSchema, cancelCloseSchema, partySchema, invoiceSchema, invoicePostingSchema, confirmPaymentSchema, taxCodeSchema, budgetSchema, leaseSchema, loanSchema, revenueContractSchema, revenueObligationSchema, type ReportDates, type CloseReconciliation, type FinanceRow, type HistoricalAgingRow } from '../domain/finance';
export { moneySchema, createJournalSchema, openingTrialBalanceSchema, createChartSchema, createAccountSchema, changeAccountStatusSchema, createPeriodSchema, closeProofSchema, reversePaymentSchema, journalBatchSchema, reportDateSchema, prepareFxRevaluationSchema, decideFxRevaluationSchema, cancelCloseSchema, partySchema, invoiceSchema, invoicePostingSchema, confirmPaymentSchema, taxCodeSchema, budgetSchema, leaseSchema, loanSchema, revenueContractSchema, revenueObligationSchema } from '../domain/finance';
export type { JournalLineInput, ReportDates, CloseReconciliation } from '../domain/finance';
import { fiscalCloseSourceSql } from "@/server/modules/finance/public/finance-fiscal-close-source";
import { readListPage } from "@/server/modules/work_projection/public/list-page";
import { prepareFlowCommand } from "@/server/modules/workflow/public/flow-routing";
import { invoiceHistoryCte } from "@/server/modules/finance/public/finance-history";
import { calculateVatCloseEvidence, pendingVatReviewSql, vatCloseGuards } from "@/server/modules/finance/public/finance-tax-register";
import { financeExpansionClosePending, financeExpansionCloseCountSql } from "@/server/modules/finance/public/finance-expansion-close";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { hasPermission } from "@/lib/identity_access/permissions";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { z } from "zod";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { ApiError } from "@/server/modules/runtime/public/http";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { prepareAttachmentBinding } from "@/server/modules/files/public/attachments/service";
import { companyRates, snapshotMoney, snapshotMoneyCarry, snapshotMoneyReversal } from "@/server/modules/finance/public/company-money";
import {fieldServiceInvoiceMoneySource} from "../../field_service/public/finance-record-sources";
import { requireFinanceMoneyScale } from "@/server/modules/finance/public/finance-money-policy";
import { nativeInvoicePosting, prepareInvoiceLineRevision } from "@/server/modules/finance/public/finance-invoice-lines";
import { readyPaymentWithholding, prepareWithholdingConsumption, prepareWithholdingReversal } from "@/server/modules/finance/public/finance-withholding";
/** Money crossing an API boundary is a decimal string. Storage uses integer minor units. */
export { minorToDecimal } from "@/lib/finance/finance-money";
export async function confirmedFunctionalFx(write: SqlDatabase, actor: WorkContext, currency: string, baseCurrency: string, date: string) { return createFunctionalFx({ rates: (companyId) => companyRates(write, companyId), error: (...args) => new ApiError(...args) })(actor, currency, baseCurrency, date); }
async function confirmedFunctionalFxMicros(write: SqlDatabase, actor: WorkContext, currency: string, baseCurrency: string, date: string) {
    return (await confirmedFunctionalFx(write, actor, currency, baseCurrency, date)).micros;
}
async function legacyMoneySnapshot(write: SqlDatabase, actor: WorkContext, input: {
    sourceType: string;
    sourceId: string;
    amountKind: string;
    transactionDate: string;
    currency: string;
    amountUnits: number;
}): Promise<SqlStatement> {
    // This adapter preserves the original two-decimal ledger meaning. New postings
    // still require a dated company quote and the shared immutable money evidence.
    await requireFinanceMoneyScale(write, actor.companyId, input.currency);
    return (await snapshotMoney(write, actor, { ...input, amountUnits: String(input.amountUnits), currencyScale: 2 })).statement;
}
async function hash(value: unknown) {
    const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(JSON.stringify(value)));
    return Array.from(new Uint8Array(bytes), b => b.toString(16).padStart(2, "0")).join("");
}
async function priorCommand<Result=Record<string,unknown>>(write:SqlDatabase,actor:WorkContext,id:string,operation:string,input:unknown){return createFinancePrior<Result>({error:(...args)=>new ApiError(...args),receiptRecordId:async key=>(await financeCommandReceipts(write).record(key))?.record_id??null,legacyCommandExists:async key=>!!await write.prepare('SELECT 1 FROM finance_commands WHERE command_id=?').bind(key).first(),replay:async(actor,key,operation,id,input)=>replayCommand<Result>(write,actor,key,operation,id,input)})(actor,id,operation,input);}
async function saveCommand<Result extends Record<string,unknown>>(write: SqlDatabase, actor: WorkContext, id: string, operation: string, input: unknown, result: Result, statements: SqlStatement[]) {
    return executeCommand(write, actor, id, operation, input, String(result.id ?? result.chartId ?? id), Number(result.version ?? 1), result, statements);
}
async function resources(actor:WorkContext,readOnly=false){createFinanceResourcePolicy({error:(...args)=>new ApiError(...args),hasPermission})(actor,readOnly);return holdingResources(actor.holdingId);}
async function financeOwnedDatabase(actor: WorkContext, provided?: SqlDatabase, readOnly = false) {
    const { write } = await resources(actor, readOnly);
    if (provided) {
        const owner = write as SqlDatabase & { assertActiveTransaction?: (database: SqlDatabase) => void };
        if (!owner.assertActiveTransaction) throw new ApiError(503, "TRANSACTION_OWNERSHIP_REQUIRED", "Finance requires its owned Holding transaction.");
        owner.assertActiveTransaction(provided);
    }
    return provided ?? write;
}
async function purchasedFinanceRead<Result>(actor: WorkContext, run: (database?: SqlDatabase) => Promise<Result>, capability = "finance.accounting") {
    if (await financePackageEnforcementMode() === "disabled") return run();
    const { write } = await resources(actor, true);
    await requireFinancePurchasedRead(write, actor, capability);
    return run(write);
}
async function purchasedFinanceReferenceRead<Result>(actor: WorkContext, run: (database?: SqlDatabase) => Promise<Result>) {
    if (await financePackageEnforcementMode() === "disabled") return run();
    const { write } = await resources(actor, true);
    await requireFinanceReferenceRead(write, actor);
    return run(write);
}
async function ensurePeriod(write: SqlDatabase, actor: WorkContext, periodId: string, entryDate: string, allowClosing = false) { return createEnsurePeriod({ error: (...args) => new ApiError(...args), period: (companyId, periodId) => write.prepare('SELECT * FROM finance_periods WHERE id=? AND company_id=?').bind(periodId, companyId).first<FinanceRow>(), coveredDate: async (companyId, date) => !!await write.prepare("SELECT 1 FROM finance_periods WHERE company_id=? AND state IN ('closed','closing') AND ? BETWEEN start_date AND end_date LIMIT 1").bind(companyId, date).first() })(actor, periodId, entryDate, allowClosing); }
export async function listCharts(actor: WorkContext, provided?:SqlDatabase) { return provided ? financeReadService(provided).listCharts(actor) : purchasedFinanceReferenceRead(actor, db => financeReadService(db).listCharts(actor)); }
export async function createChart(actor: WorkContext, input: z.infer<typeof createChartSchema>, id: string) { return withFinanceConfigurationCommand(actor,{key:id,operation:"finance.chart.create",purpose:"new-work"},tx=>configurationService(tx).createChart(actor,input,id)); }
export async function createSupplyPostingChart(actor:WorkContext,raw:unknown,key:string){const request=supplyPostingChartSchema.parse(raw);return withSupplyPostingPrerequisiteCommand(actor,'chart',request,key,tx=>configurationService(tx,request).createChart(actor,request.input,key));}
export async function createSalesPostingChart(actor:WorkContext,raw:unknown,key:string){const request=salesPostingChartSchema.parse(raw);return withSalesPostingCommand(actor,'chart',request,key,tx=>configurationService(tx,request).createChart(actor,request.input,key));}
export async function listAccounts(actor: WorkContext, chartId: string) { return purchasedFinanceReferenceRead(actor, db => financeReadService(db).listAccounts(actor, chartId)); }
export async function createAccount(actor: WorkContext, input: z.infer<typeof createAccountSchema>, id: string) { return withFinanceConfigurationCommand(actor,{key:id,operation:"finance.account.create",purpose:"new-work"},tx=>configurationService(tx).createAccount(actor,input,id)); }
export async function createSupplyPostingAccount(actor:WorkContext,raw:unknown,key:string){const request=supplyPostingAccountSchema.parse(raw);return withSupplyPostingPrerequisiteCommand(actor,'account',request,key,tx=>configurationService(tx,request).createAccount(actor,request.input,key));}
export async function createSalesPostingAccount(actor:WorkContext,raw:unknown,key:string){const request=salesPostingAccountSchema.parse(raw);return withSalesPostingCommand(actor,'account',request,key,tx=>configurationService(tx,request).createAccount(actor,request.input,key));}
export async function changeAccountStatus(actor: WorkContext, chartId: string, accountId: string, input: z.infer<typeof changeAccountStatusSchema>, id: string) { return withFinanceConfigurationCommand(actor,{key:id,operation:`finance.account.${input.status}`,purpose:input.status==="active"?"new-work":"settle-existing"},tx=>configurationService(tx).changeAccountStatus(actor,chartId,accountId,input,id)); }
export async function importAccounts(actor: WorkContext, chartId: string, rows: Array<{
    code: string;
    name: string;
    category: z.infer<typeof createAccountSchema>["category"];
    currency?: string;
    controlKind?: "none" | "receivable" | "payable" | "bank";
}>, id: string) { return withFinanceConfigurationCommand(actor,{key:id,operation:"finance.account.import",purpose:"new-work"},tx=>accountImportService(tx)(actor,chartId,rows,id)); }
export async function createPeriod(actor: WorkContext, input: z.infer<typeof createPeriodSchema>, id: string) { return withFinanceConfigurationCommand(actor,{key:id,operation:"finance.period.create",purpose:"new-work"},tx=>configurationService(tx).createPeriod(actor,input,id)); }
export async function createSupplyPostingPeriod(actor:WorkContext,raw:unknown,key:string){const request=supplyPostingPeriodSchema.parse(raw);return withSupplyPostingPrerequisiteCommand(actor,'period',request,key,tx=>configurationService(tx,request).createPeriod(actor,request.input,key));}
export async function createSalesPostingPeriod(actor:WorkContext,raw:unknown,key:string){const request=salesPostingPeriodSchema.parse(raw);return withSalesPostingCommand(actor,'period',request,key,tx=>configurationService(tx,request).createPeriod(actor,request.input,key));}
export async function prepareJournal(write: SqlDatabase, actor: WorkContext, input: z.infer<typeof createJournalSchema>, key: string, status: "draft" | "posted" = "draft") { const plan = await journalPreparation(write)(actor, input, key, status); return journalStatements(write, actor, input, key, status, plan); }
export async function createJournal(actor: WorkContext, input: z.infer<typeof createJournalSchema>, id: string) { return withFinancePurchasedCommand(actor,{key:id,operation:"finance.journal.create",purpose:"new-work"},tx=>openingService(tx).createJournal(actor,input,id)); }
export async function previewOpeningTrialBalance(actor: WorkContext, input: z.infer<typeof openingTrialBalanceSchema>) { return purchasedFinanceRead(actor,()=>openingService().previewOpeningTrialBalance(actor,input)); }
export async function importOpeningTrialBalance(actor: WorkContext, input: z.infer<typeof openingTrialBalanceSchema> & {
    previewToken: string;
}, id: string) { return withFinancePurchasedCommand(actor,{key:id,operation:"finance.journal.create",purpose:"new-work"},tx=>openingService(tx).importOpeningTrialBalance(actor,input,id)); }
export async function listJournals(actor: WorkContext, query: {
    periodId?: string;
    status?: string;
}) { return purchasedFinanceRead(actor,db=>financeReadService(db).listJournals(actor,query)); }
export async function listInvoices(actor: WorkContext, limit = 100) { return purchasedFinanceRead(actor,db=>financeReadService(db).listInvoices(actor,limit)); }
export async function workInvoiceRows(actor: WorkContext, provided?:SqlDatabase) {
    if (await financePackageEnforcementMode() !== "disabled") {
        try { await requireFinancePurchasedRead(provided ?? (await resources(actor,true)).write, actor); }
        catch (error) { if (error instanceof ApiError && error.code === "PACKAGE_REQUIRED") return []; throw error; }
    }
    return financeReadService(provided).workInvoiceRows(actor);
}
export async function getInvoice(actor: WorkContext, invoiceId: string, provided?:SqlDatabase) { const db=provided??(await holdingResources(actor.holdingId)).write;await requireFinanceDocumentHistory(db,actor,"invoice",invoiceId);return financeReadService(db).getInvoice(actor,invoiceId); }
export async function listPayments(actor: WorkContext, limit = 100) { return purchasedFinanceRead(actor,db=>financeReadService(db).listPayments(actor,limit),"finance.banking"); }
export async function getPayment(actor: WorkContext, paymentId: string) { const db=(await holdingResources(actor.holdingId)).write,access=await requireFinanceDocumentHistory(db,actor,"payment",paymentId);const sourcePurchase=await servicePaymentPurchase(db,actor,{kind:"payment",id:paymentId})??await supplierPaymentCorrectionPurchase(db,actor,{kind:"payment",id:paymentId})??await salesReceiptReversalPurchase(db,actor,paymentId);const result=await financeReadService(db,!(access.canSettle||sourcePurchase?.settleExisting)).getPayment(actor,paymentId);
 if(sourcePurchase&&access.capability==="finance.service-invoice"){
  const rows=(await db.prepare("SELECT t.id FROM finance_bank_transactions t JOIN command_receipts r ON r.record_id=t.statement_id AND r.operation='finance.bank.import' AND r.authority_json::jsonb->>'companyId'=t.company_id AND r.result_json::jsonb->>'settlementPaymentId'=? WHERE t.company_id=?").bind(paymentId,actor.companyId).all<{id:string}>()).results;const ids=new Set(rows.map(row=>row.id));result.bankTransactions=result.bankTransactions.filter(row=>ids.has(String(row.id)));
 }
 if(sourcePurchase?.capability==="finance.inventory-valuation"&&access.capability==="finance.inventory-valuation"){
  const source=await supplierPaymentCorrectionSourceIds(db,actor,paymentId);result.invoices=result.invoices.filter(row=>row.id===source?.invoiceId);result.bankTransactions=result.bankTransactions.filter(row=>row.id===source?.bankTransactionId);
 }
 if(sourcePurchase?.capability==='finance.sales-posting'&&access.capability==='finance.sales-posting'){result.invoices=[];result.bankTransactions=[];result.fxAccounts=[];}
 return {...result,...(sourcePurchase?{sourcePurchase}:{})}; }
export async function reversePayment(actor: WorkContext, paymentId: string, input: z.infer<typeof reversePaymentSchema>, id: string) {
 const original=()=>withSupplierPaymentCorrection(actor,{kind:"payment",id:paymentId},{key:id,operation:"finance.payment.reverse"},tx=>paymentService(tx).reversePayment(actor,paymentId,input,id),()=>withServicePaymentCommand(actor,{kind:"payment",id:paymentId},{key:id,operation:"finance.payment.reverse",purpose:"settle-existing"},tx=>paymentService(tx).reversePayment(actor,paymentId,input,id)));
 return withSalesReceiptReversal(actor,paymentId,id,tx=>paymentService(tx).reversePayment(actor,paymentId,input,id),()=>withSupplierReversalMoney(actor,paymentId,input,id,database=>withSupplierPaymentCorrection(actor,{kind:"payment",id:paymentId},{key:id,operation:"finance.payment.reverse"},()=>paymentService(database).reversePayment(actor,paymentId,input,id),original,database),original));
}
export async function postJournal(actor: WorkContext, journalId: string, id: string, input: {
    expectedVersion: number;
}) { return withFinanceDocumentPost(actor,"journal",journalId,{key:id,operation:"finance.journal.post"},tx=>journalReviewService(tx).postJournal(actor,journalId,id,input)); }
export async function postJournalBatch(actor: WorkContext, input: z.input<typeof journalBatchSchema>, id: string) { return withFinancePurchasedCommand(actor,{key:id,operation:"finance.batch.post",purpose:"settle-existing"},tx=>journalBatchService(tx)(actor,input,id)); }
export async function reverseJournal(actor: WorkContext, journalId: string, id: string, input: {
    periodId: string;
    entryDate: string;
    memo?: string;
}) { return withJournalReversalMoneyUnit(actor,journalId,id,input.periodId,database=>withFinancePurchasedCommand(actor,{key:id,operation:"finance.journal.reverse",purpose:"settle-existing"},()=>journalReviewService(database).reverseJournal(actor,journalId,id,input),database)); }
export async function trialBalance(actor: WorkContext, periodId?: string, chartId?: string, dates: ReportDates = {}, options: {
    excludeYearClose?: boolean;
} = {}) { return purchasedFinanceRead(actor,db=>financeReadService(db).trialBalance(actor,periodId,chartId,dates,options)); }
export async function statements(actor: WorkContext, periodId?: string, chartId?: string, dates: ReportDates = {}, provided?:SqlDatabase) { return provided ? financeReadService(provided).statements(actor,periodId,chartId,dates) : purchasedFinanceRead(actor,db=>financeReadService(db).statements(actor,periodId,chartId,dates)); }
export async function calculateCloseReconciliations(write: SqlDatabase, actor: WorkContext, period: FinanceRow) { return closeService(write).calculateCloseReconciliations(actor, period); }
function reconciliationAssertions(write: SqlDatabase, actor: WorkContext, period: FinanceRow, key: string, rows: CloseReconciliation[]) {
    return rows.flatMap((row, index) => {
        const sourceAccountId = String(row.source.sourceAccountId ?? row.accountId);
        const signedLedger = row.kind === "payable" ? -row.ledgerMinor : row.ledgerMinor;
        const checks = [assertion(write, key, `reconciliation-ledger-${index}`, "COALESCE((SELECT SUM(l.base_debit_minor-l.base_credit_minor) FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND l.account_id=? AND j.status IN ('posted','reversed') AND j.entry_date<=?),0)=?", [actor.companyId, row.accountId, period.end_date, signedLedger])];
        if (row.kind === "bank")
            checks.push(assertion(write, key, `reconciliation-bank-${index}`, "EXISTS(SELECT 1 FROM finance_bank_statements WHERE company_id=? AND bank_account=? AND statement_date=? AND CASE WHEN base_closing_minor=0 AND closing_minor!=0 AND closing_fx_rate_micros=1000000 THEN closing_minor ELSE base_closing_minor END=?)", [actor.companyId, sourceAccountId, period.end_date, row.sourceMinor]));
        else {
            checks.push(assertion(write, key, `reconciliation-subledger-${index}`, `${invoiceHistoryCte}
        SELECT COALESCE(SUM(historical_base_outstanding_minor),0)=? AND COUNT(*) FILTER(WHERE legacy_date_evidence)=0 FROM historical_invoice_balances
        WHERE kind=? AND CASE WHEN kind='receivable' THEN debit_account_id ELSE credit_account_id END=?`, [actor.companyId, period.end_date, row.sourceMinor, row.kind, sourceAccountId]));
            checks.push(assertion(write, key, `reconciliation-foreign-${index}`, `${invoiceHistoryCte}
        SELECT COUNT(*)=? FROM historical_invoice_balances i JOIN finance_charts c ON c.id=i.chart_id
        WHERE i.kind=? AND CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END=?
          AND i.currency<>c.base_currency AND i.historical_outstanding_minor>0 AND NOT EXISTS(
            SELECT 1 FROM finance_fx_revaluation_lines l JOIN finance_fx_revaluation_runs r ON r.id=l.run_id
            WHERE l.invoice_id=i.id AND r.period_id=? AND r.status='posted')`, [actor.companyId, period.end_date, Number(row.source.foreignOpenCount ?? 0), row.kind, sourceAccountId, period.id]));
        }
        return checks;
    });
}
export async function fxRevaluationWorkspace(actor: WorkContext, periodId: string) { return purchasedFinanceRead(actor,db=>fxService(db).fxRevaluationWorkspace(actor,periodId),"finance.close"); }
export async function prepareFxRevaluation(actor: WorkContext, input: z.input<typeof prepareFxRevaluationSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.fx-revaluation.prepare",purpose:"settle-existing",capability:"finance.close"},tx=>fxService(tx).prepareFxRevaluation(actor,input,key)); }
export async function decideFxRevaluation(actor: WorkContext, runId: string, input: z.input<typeof decideFxRevaluationSchema>, decision: "approve" | "reject", key: string) { return withFinancePurchasedCommand(actor,{key,operation:`finance.fx-revaluation.${decision}`,purpose:"settle-existing",capability:"finance.close"},tx=>fxService(tx).decideFxRevaluation(actor,runId,input,decision,key)); }
function refundCloseGuards(write: SqlDatabase, actor: WorkContext, period: FinanceRow, key: string) {
    return [
        financeCloseProcurementSource(write).guard(key,actor.companyId,period.end_date),
        assertion(write, key, "unresolved-supplier-bank", "NOT EXISTS(SELECT 1 FROM finance_supplier_instructions i JOIN finance_supplier_batches b ON b.id=i.batch_id WHERE b.company_id=? AND i.status IN ('exported','unknown') AND b.planned_date<=?)", [actor.companyId, period.end_date]),
        financeCloseCommerceSource(write).guard(key,actor.companyId,period.end_date),
        assertion(write, key, "unresolved-refund-bank", "NOT EXISTS(SELECT 1 FROM finance_refund_instructions WHERE company_id=? AND status IN ('exported','unknown') AND planned_date<=?)", [actor.companyId, period.end_date]),
    ];
}
export async function beginClose(actor: WorkContext, periodId: string, id: string) { return withFinancePurchasedCommand(actor,{key:id,operation:"finance.period.begin-close",purpose:"settle-existing",capability:"finance.close"},tx=>closeService(tx).beginClose(actor,periodId,id)); }
export async function cancelClose(actor: WorkContext, periodId: string, input: z.infer<typeof cancelCloseSchema>, id: string) { return withFinancePurchasedCommand(actor,{key:id,operation:"finance.period.cancel-close",purpose:"settle-existing",capability:"finance.close"},tx=>closeService(tx).cancelClose(actor,periodId,input,id)); }
export async function closePeriod(actor: WorkContext, periodId: string, input: z.input<typeof closeProofSchema>, id: string) { return withFinancePurchasedCommand(actor,{key:id,operation:"finance.period.close",purpose:"settle-existing",capability:"finance.close"},tx=>closeService(tx).closePeriod(actor,periodId,input,id)); }
export async function reopenPeriod(actor: WorkContext, periodId: string, input: z.input<typeof closeProofSchema>, id: string) { return withFinancePurchasedCommand(actor,{key:id,operation:"finance.period.reopen",purpose:"settle-existing",capability:"finance.close"},tx=>closeService(tx).reopenPeriod(actor,periodId,input,id)); }
export async function createParty(actor: WorkContext, input: z.infer<typeof partySchema>, id: string) { return withFinanceConfigurationCommand(actor,{key:id,operation:"finance.party.create",purpose:"new-work"},tx=>configurationService(tx).createParty(actor,input,id)); }
export async function createInvoice(actor: WorkContext, input: z.input<typeof invoiceSchema>, id: string, existingId?: string, ownedTransaction?: SqlDatabase) { return ownedTransaction ? invoiceService(ownedTransaction).createInvoice(actor,input,id,existingId) : withFinancePurchasedCommand(actor,{key:id,operation:existingId?"finance.invoice.update":"finance.invoice.create",purpose:"new-work"},tx=>invoiceService(tx).createInvoice(actor,input,id,existingId)); }
export async function voidDraftInvoice(actor: WorkContext, invoiceId: string, expectedVersion: number, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.invoice.void",purpose:"settle-existing"},tx=>invoiceService(tx).voidDraftInvoice(actor,invoiceId,expectedVersion,key)); }
export async function invoiceWorkspace(actor: WorkContext) { return purchasedFinanceRead(actor,db=>financeReadService(db).invoiceWorkspace(actor)); }
export async function postInvoice(actor: WorkContext, invoiceId: string, input: z.input<typeof invoicePostingSchema>, id: string, ownedTransaction?: SqlDatabase) { return ownedTransaction ? invoiceService(ownedTransaction).postInvoice(actor,invoiceId,input,id) : withFinanceDocumentPost(actor,"invoice",invoiceId,{key:id,operation:"finance.invoice.post"},tx=>invoiceService(tx).postInvoice(actor,invoiceId,input,id)); }
export async function aging(actor: WorkContext, kind: "payable" | "receivable", asOf: string, chartId?: string) { return purchasedFinanceRead(actor,db=>financeReadService(db).aging(actor,kind,asOf,chartId)); }
export async function createPayment(actor: WorkContext, input: {
    partyId?: string;
    kind: "payment" | "receipt" | "refund";
    paymentNumber: string;
    paymentDate: string;
    currency: string;
    amount: string;
    bankAccount?: string;
    settlementInvoiceId?: string;
    correctionPaymentId?: string;
}, id: string) { if(input.correctionPaymentId){if(!input.settlementInvoiceId)throw new ApiError(422,"SUPPLIER_CORRECTION_SOURCE_REQUIRED","Choose the original purchasing invoice for this correction.");return withSupplierPaymentCorrection(actor,{kind:"create",invoiceId:input.settlementInvoiceId,correctionPaymentId:input.correctionPaymentId},{key:id,operation:"finance.payment.create"},tx=>paymentService(tx).createPayment(actor,input,id),()=>withFinancePurchasedCommand(actor,{key:id,operation:"finance.payment.create",purpose:"settle-existing",capability:"finance.banking"},tx=>paymentService(tx).createPayment(actor,input,id)));}return withServicePaymentCommand(actor,input.settlementInvoiceId?{kind:"invoice",id:input.settlementInvoiceId}:undefined,{key:id,operation:"finance.payment.create",purpose:input.settlementInvoiceId?"settle-existing":"new-work"},tx=>paymentService(tx).createPayment(actor,input,id)); }
export async function approvePayment(actor: WorkContext, paymentId: string, input: {
    expectedVersion: number;
    proof: string;
}, id: string) { return withSupplierPaymentCorrection(actor,{kind:"payment",id:paymentId},{key:id,operation:"finance.payment.approve"},tx=>paymentService(tx).approvePayment(actor,paymentId,input,id),()=>withServicePaymentCommand(actor,{kind:"payment",id:paymentId},{key:id,operation:"finance.payment.approve",purpose:"settle-existing"},tx=>paymentService(tx).approvePayment(actor,paymentId,input,id))); }
export async function releasePayment(actor: WorkContext, paymentId: string, input: {
    expectedVersion: number;
    proof: string;
}, id: string) { return withSupplierPaymentCorrection(actor,{kind:"payment",id:paymentId},{key:id,operation:"finance.payment.release"},tx=>paymentService(tx).releasePayment(actor,paymentId,input,id),()=>withServicePaymentCommand(actor,{kind:"payment",id:paymentId},{key:id,operation:"finance.payment.release",purpose:"settle-existing"},tx=>paymentService(tx).releasePayment(actor,paymentId,input,id))); }
export async function confirmPayment(actor: WorkContext, paymentId: string, input: z.input<typeof confirmPaymentSchema>, key: string) {
 const run=async(owned?:SqlDatabase)=>{const confirm=async(tx:SqlDatabase)=>{const result=await paymentService(tx).confirmPayment(actor,paymentId,input,key);await ensureCustomerReceipt(tx,actor,paymentId);return result;};if(owned)return confirm(owned);return financeTransaction((await resources(actor)).write,confirm);};
 return withSupplierPaymentCorrection(actor,{kind:"payment",id:paymentId},{key,operation:"finance.payment.confirm",bankTransactionId:input.bankTransactionId},run,()=>withServicePaymentCommand(actor,{kind:"payment",id:paymentId},{key,operation:"finance.payment.confirm",purpose:"settle-existing",bankTransactionId:input.bankTransactionId},run));
}
export async function allocatePayment(actor: WorkContext, paymentId: string, input: {
    invoiceId: string;
    amount: string;
    expectedVersion: number;
}, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.payment.allocate",purpose:"settle-existing",capability:"finance.banking"},tx=>paymentService(tx).allocatePayment(actor,paymentId,input,key)); }
export async function exportPaymentFile(actor: WorkContext, paymentIds: string[], id: string) { return withFinancePurchasedCommand(actor,{key:id,operation:"finance.payment.export",purpose:"history",capability:"finance.banking"},tx=>paymentService(tx).exportPaymentFile(actor,paymentIds,id)); }
export async function bankWorkspace(actor: WorkContext, query?: {
    print?: string;
    transactionCursor?: string;
    statementCursor?: string;
    match?: string;
    new?: string;
    chart?: string;
    view?: string;
    q?: string;
}) { return purchasedFinanceRead(actor,db=>financeReadService(db).bankWorkspace(actor,query),"finance.banking"); }
export async function importBankStatement(actor: WorkContext, input: {
    settlementPaymentId?: string;
    bankAccount: string;
    statementNumber: string;
    statementDate: string;
    opening: string;
    closing: string;
    closingFxRate?: string;
    sourceHash: string;
    rows: Array<{
        externalId: string;
        transactionDate: string;
        description?: string;
        amount: string;
        currency: string;
    }>;
}, key: string, ownedTransaction?: SqlDatabase) {
 if(input.settlementPaymentId){
  if(ownedTransaction)throw new ApiError(503,"TRANSACTION_OWNERSHIP_REQUIRED","Source-scoped bank evidence uses the normal controlled settlement owner.");
  return withServicePaymentCommand(actor,{kind:"payment",id:input.settlementPaymentId},{key,operation:"finance.bank.import",purpose:"settle-existing"},async transaction=>{
   const write=transaction??(await resources(actor)).write;
   const prior=await replayCommand(write,actor,key,"finance.bank.import",`statement-${key}`,input);if(prior)return prior;
   await requireServiceBankEvidence(write,actor,input);
   return bankService(write).importBankStatement(actor,input,key);
  });
 }
 return ownedTransaction ? bankService(ownedTransaction).importBankStatement(actor,input,key) : withFinancePurchasedCommand(actor,{key,operation:"finance.bank.import",purpose:"settle-existing",capability:"finance.banking"},tx=>bankService(tx).importBankStatement(actor,input,key));
}
/** Exact instruction-scoped entry; transport cannot supply a trusted marker. */
export async function importRefundBankEvidence(actor:WorkContext,instructionId:string,raw:unknown,key:string){
 const input=refundBankEvidenceSchema.parse(raw);
 return withRefundBankEvidenceImport(actor,instructionId,input,key,(database,witness,bankInput)=>bankService(database,witness).importBankStatement(actor,bankInput,key));
}
/** Closed Payroll owner composition; validates the actual private unit before any import. */
export function importPayrollOtherBankEvidenceInOwnedUnit(database:SqlDatabase,actor:WorkContext,runId:string,input:PayrollOtherBankEvidenceInput,key:string,contextKey:string){return withPayrollOtherBankEvidence(database,actor,runId,input,key,contextKey,(write,witness,bankInput)=>bankService(write,undefined,witness).importBankStatement(actor,bankInput,key));}
async function requireServiceBankEvidence(write:SqlDatabase,actor:WorkContext,input:BankImportInput){
 const payment=await write.prepare(`SELECT p.* FROM finance_payments p JOIN (${fieldServiceFinanceRecordSources.payment}) actual ON actual.id=p.id AND actual.company_id=p.company_id WHERE p.id=? AND p.company_id=?`).bind(input.settlementPaymentId!,actor.companyId).first<FinanceRow>();
 const row=input.rows[0];
 if(!payment||payment.status!=="released"||input.rows.length!==1||input.bankAccount!==payment.bank_account||row.currency.toUpperCase()!==payment.currency||decimalToMinor(row.amount)!==Number(payment.amount_minor)||row.transactionDate<String(payment.payment_date))throw new ApiError(422,"SERVICE_PAYMENT_BANK_SOURCE_INVALID","Import one exact bank receipt for the released Service payment's account, currency, date and amount.");
}
export async function reconcileBankTransaction(actor: WorkContext, transactionId: string, input: {
    expectedVersion: number;
    matchedType: string;
    matchedId: string;
    proof: string;
    attachmentSetIds: string[];
}, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.bank.reconcile",purpose:"settle-existing",capability:"finance.banking"},tx=>bankService(tx).reconcileBankTransaction(actor,transactionId,input,key)); }
export async function createTaxCode(actor: WorkContext, input: z.infer<typeof taxCodeSchema>, id: string) { return withFinanceConfigurationCommand(actor,{key:id,operation:"finance.tax-code.create",purpose:"new-work"},tx=>configurationService(tx).createTaxCode(actor,input,id)); }
export { assetSchema, createAsset, depreciateAsset } from "@/server/modules/finance/public/finance-assets";
export async function createBudget(actor: WorkContext, input: z.infer<typeof budgetSchema>, id: string) { return withFinancePurchasedCommand(actor,{key:id,operation:"finance.budget.create",purpose:"new-work"},tx=>configurationService(tx).createBudget(actor,input,id)); }
export async function createLease(actor: WorkContext, input: z.infer<typeof leaseSchema>, id: string) { return withFinancePurchasedCommand(actor,{key:id,operation:"finance.lease.create",purpose:"new-work",capability:"finance.advanced"},tx=>configurationService(tx).createLease(actor,input,id)); }
export async function createLoan(actor: WorkContext, input: z.infer<typeof loanSchema>, id: string) { return withFinancePurchasedCommand(actor,{key:id,operation:"finance.loan.create",purpose:"new-work",capability:"finance.advanced"},tx=>configurationService(tx).createLoan(actor,input,id)); }
export async function createRevenueContract(actor: WorkContext, input: z.infer<typeof revenueContractSchema>, id: string) { return withFinancePurchasedCommand(actor,{key:id,operation:"finance.revenue-contract.create",purpose:"new-work"},tx=>configurationService(tx).createRevenueContract(actor,input,id)); }
export async function createRevenueObligation(actor: WorkContext, input: z.infer<typeof revenueObligationSchema>, id: string) { return withFinancePurchasedCommand(actor,{key:id,operation:"finance.revenue-obligation.create",purpose:"new-work"},tx=>configurationService(tx).createRevenueObligation(actor,input,id)); }
export async function recognizeRevenue(actor: WorkContext, obligationId: string, input: {
    amount: string;
    periodId: string;
    entryDate: string;
}, id: string) { return legacyRevenueService()(actor, obligationId, input, id); }
export async function financeOverviewCounts(actor: WorkContext) { return purchasedFinanceRead(actor,db=>financeReadService(db).financeOverviewCounts(actor)); }
export async function invoicesPage(actor: WorkContext, query: {
    q?: string;
    kind?: string;
    status?: string;
    queue?: string;
    cursor?: string;
    chartId?: string;
}) { return purchasedFinanceRead(actor,db=>financeReadService(db).invoicesPage(actor,query)); }
/** Read a statement by ID without depending on the current register cursor. */
export async function bankStatementDetail(actor: WorkContext, id: string, page = 1) { return purchasedFinanceRead(actor,db=>financeReadService(db).bankStatementDetail(actor,id,page),"finance.banking"); }
function journalPreparation(write: SqlDatabase) {
    return createJournalPreparation({ error: (...args) => new ApiError(...args), now: Date.now, chart: (companyId, chartId) => write.prepare("SELECT id,base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'").bind(chartId, companyId).first<{
            id: string;
            base_currency: string;
        }>(), period: (actor, periodId, date) => ensurePeriod(write, actor, periodId, date), moneyScale: (companyId, currency) => requireFinanceMoneyScale(write, companyId, currency), functionalFx: (actor, currency, baseCurrency, date) => confirmedFunctionalFx(write, actor, currency, baseCurrency, date), accounts: async (companyId, chartId, accountIds) => (await write.prepare(`SELECT id FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND id IN (${accountIds.map(() => "?").join(",")})`).bind(companyId, chartId, ...accountIds).all<{
            id: string;
        }>()).results, openingExists: async (companyId, chartId) => !!await write.prepare("SELECT id FROM finance_journals WHERE company_id=? AND chart_id=? AND is_opening=1 AND status IN ('draft','in_review','posted') LIMIT 1").bind(companyId, chartId).first() });
}
function configurationService(provided?: SqlDatabase,boundRequest?:unknown) {
    let pending: Promise<SqlDatabase> | undefined;
    const requestIdentity=(input:unknown)=>boundRequest??input;
    const db = (actor: WorkContext) => pending ??= financeOwnedDatabase(actor, provided);
    return createFinanceConfiguration({ error: (...args) => new ApiError(...args), authorize: requirePermission, now: Date.now, id: () => crypto.randomUUID(), ready: async (actor) => { await db(actor); }, prior: async <Result>(actor:WorkContext,id:string,operation:string,input:unknown) => priorCommand<Result>(await db(actor), actor, id, operation, requestIdentity(input)), moneyScale: async (companyId, currency) => requireFinanceMoneyScale(await pending!, companyId, currency), commitCreateChart: async (facts) => {
            const { chartId, actor, input, now, id, result } = facts;
            const write = await db(actor);
            const pendingStatements: SqlStatement[] = [];
            pendingStatements.push(write.prepare("INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)").bind(chartId, actor.companyId, input.name, input.baseCurrency, input.accountingBasis, actor.userId, now, now));
            return saveCommand(write, actor, id, "finance.chart.create", requestIdentity(input), result, pendingStatements);
        },
        accountChart: async (facts) => {
            const { input, actor } = facts;
            const write = await db(actor);
            return write.prepare("SELECT id,base_currency FROM finance_charts WHERE id=? AND company_id=?").bind(input.chartId, actor.companyId).first<{
                id: string;
                base_currency: string;
            }>();
        },
        commitCreateAccount: async (facts) => {
            const { accountId, input, actor, chart, now, id, result } = facts;
            const write = await db(actor);
            const pendingStatements: SqlStatement[] = [];
            pendingStatements.push(write.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,parent_id,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(accountId, input.chartId, actor.companyId, input.code, input.name, input.category, input.parentId ?? null, input.currency ?? chart.base_currency, input.isControl || input.controlKind !== "none" ? 1 : 0, input.controlKind, now, now));
            return saveCommand(write, actor, id, "finance.account.create", requestIdentity(input), result, pendingStatements);
        },
        statusAccount: async (facts) => {
            const { accountId, chartId, actor } = facts;
            const write = await db(actor);
            return write.prepare("SELECT id,status,version,is_control FROM finance_accounts WHERE id=? AND chart_id=? AND company_id=?").bind(accountId, chartId, actor.companyId).first<{
                id: string;
                status: string;
                version: number;
                is_control: number;
            }>();
        },
        draftAccountUsage: async (facts) => { const { accountId, actor } = facts; const write = await db(actor); return !!await write.prepare("SELECT 1 FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.account_id=? AND j.company_id=? AND j.status='draft' LIMIT 1").bind(accountId, actor.companyId).first(); },
        openAccountUsage: async (facts) => { const { actor, accountId } = facts; const write = await db(actor); return !!await write.prepare("SELECT 1 FROM finance_invoices WHERE company_id=? AND outstanding_minor>0 AND status IN ('draft','posted','partially_paid') AND (debit_account_id=? OR credit_account_id=?) LIMIT 1").bind(actor.companyId, accountId, accountId).first(); },
        commitChangeAccountStatus: async (facts) => {
            const { actor, id, operation, chartId, accountId, input, result, account, now } = facts;
            const write = await db(actor);
            return saveCommand(write, actor, id, operation, { chartId, accountId, ...input }, result, [
                assertion(write, id, "account-version", "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND chart_id=? AND company_id=? AND status=? AND version=?)", [accountId, chartId, actor.companyId, account.status, input.expectedVersion]),
                write.prepare("UPDATE finance_accounts SET status=?,version=version+1,updated_at=? WHERE id=? AND chart_id=? AND company_id=? AND version=?").bind(input.status, now, accountId, chartId, actor.companyId, input.expectedVersion),
            ]);
        },
        commitCreatePeriod: async (facts) => { const { periodId, actor, input, now, id, result } = facts; const write = await db(actor); const pendingStatements: SqlStatement[] = []; pendingStatements.push(write.prepare("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,?,?,?,?)").bind(periodId, actor.companyId, input.name, input.startDate, input.endDate, now)); return saveCommand(write, actor, id, "finance.period.create", requestIdentity(input), result, pendingStatements); },
        commitCreateParty: async (facts) => { const { partyId, actor, input, now, id, output } = facts; const write = await db(actor); const pendingStatements: SqlStatement[] = []; pendingStatements.push(write.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,tax_registration,payment_terms_days,credit_limit_minor,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(partyId, actor.companyId, input.kind, input.code, input.name, input.currency, input.taxRegistration ?? null, input.paymentTermsDays, decimalToMinor(input.creditLimit), now, now)); return saveCommand(write, actor, id, "finance.party.create", requestIdentity(input), output, pendingStatements); },
        commitCreateTaxCode: async (facts) => { const { taxId, actor, input, rate, recoverable, id, output } = facts; const write = await db(actor); const pendingStatements: SqlStatement[] = []; pendingStatements.push(write.prepare("INSERT INTO finance_tax_codes(id,company_id,code,name,rate_micros,recoverable_micros,effective_from,effective_to,status) VALUES(?,?,?,?,?,?,?,?,?)").bind(taxId, actor.companyId, input.code, input.name, rate, recoverable, input.effectiveFrom, input.effectiveTo ?? null, "active")); return saveCommand(write, actor, id, "finance.tax-code.create", input, output, pendingStatements); },
        commitCreateBudget: async (facts) => { const { budgetId, actor, input, now, id, output } = facts; const write = await db(actor); const pendingStatements: SqlStatement[] = []; pendingStatements.push(write.prepare("INSERT INTO finance_budgets(id,company_id,name,fiscal_year,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)").bind(budgetId, actor.companyId, input.name, input.fiscalYear, input.currency, actor.userId, now, now)); return saveCommand(write, actor, id, "finance.budget.create", input, output, pendingStatements); },
        commitCreateLease: async (facts) => { const { leaseId, actor, input, payment, rate, now, id, output } = facts; const write = await db(actor); const pendingStatements: SqlStatement[] = []; pendingStatements.push(write.prepare("INSERT INTO finance_leases(id,company_id,lease_number,counterparty,currency,commencement_date,end_date,payment_frequency,payment_minor,discount_rate_micros,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(leaseId, actor.companyId, input.leaseNumber, input.counterparty, input.currency, input.commencementDate, input.endDate, input.paymentFrequency, payment, rate, actor.userId, now, now)); return saveCommand(write, actor, id, "finance.lease.create", input, output, pendingStatements); },
        commitCreateLoan: async (facts) => { const { loanId, actor, input, principal, now, id, output } = facts; const write = await db(actor); const pendingStatements: SqlStatement[] = []; pendingStatements.push(write.prepare("INSERT INTO finance_loans(id,company_id,loan_number,lender,currency,principal_minor,annual_rate_micros,commencement_date,maturity_date,outstanding_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(loanId, actor.companyId, input.loanNumber, input.lender, input.currency, principal, decimalToMinor(input.annualRate, 6), input.commencementDate, input.maturityDate, principal, actor.userId, now, now)); return saveCommand(write, actor, id, "finance.loan.create", input, output, pendingStatements); },
        revenueCustomer: async (facts) => { const { input, actor } = facts; const write = await db(actor); return !!await write.prepare("SELECT id FROM finance_parties WHERE id=? AND company_id=? AND kind='customer'").bind(input.customerId, actor.companyId).first(); },
        commitCreateRevenueContract: async (facts) => { const { contractId, actor, input, now, id, output } = facts; const write = await db(actor); const pendingStatements: SqlStatement[] = []; pendingStatements.push(write.prepare("INSERT INTO finance_revenue_contracts(id,company_id,customer_id,contract_number,currency,transaction_price_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(contractId, actor.companyId, input.customerId, input.contractNumber, input.currency, decimalToMinor(input.transactionPrice), actor.userId, now, now)); return saveCommand(write, actor, id, "finance.revenue-contract.create", input, output, pendingStatements); },
        revenueContract: async (facts) => { const { input, actor } = facts; const write = await db(actor); return !!await write.prepare("SELECT id FROM finance_revenue_contracts WHERE id=? AND company_id=?").bind(input.contractId, actor.companyId).first(); },
        commitCreateRevenueObligation: async (facts) => { const { obligationId, input, actor, id, output } = facts; const write = await db(actor); const pendingStatements: SqlStatement[] = []; pendingStatements.push(write.prepare("INSERT INTO finance_revenue_obligations(id,contract_id,description,allocated_minor,recognition_method,start_date,end_date,revenue_account_id,deferred_account_id) VALUES(?,?,?,?,?,?,?,?,?)").bind(obligationId, input.contractId, input.description, decimalToMinor(input.allocated), input.recognitionMethod, input.startDate, input.endDate ?? null, input.revenueAccountId, input.deferredAccountId)); return saveCommand(write, actor, id, "finance.revenue-obligation.create", input, output, pendingStatements); } });
}
function journalStatements(write: SqlDatabase, actor: WorkContext, input: JournalInput, key: string, status: "draft" | "posted", plan: JournalPlan) {
    const { fx, accountIds, journalId, number, now, values, preparedFxEvidence } = plan;
    const guards: SqlStatement[] = [
        assertion(write, key, "posting-period", "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)", [input.periodId, actor.companyId, input.entryDate, input.entryDate]),
        assertion(write, key, "posting-chart", "EXISTS(SELECT 1 FROM finance_charts WHERE id=? AND company_id=? AND status='active')", [input.chartId, actor.companyId]),
        ...(input.isOpening ? [assertion(write, key, "opening-balance", "NOT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=? AND chart_id=? AND is_opening=1 AND status IN ('draft','in_review','posted'))", [actor.companyId, input.chartId])] : []),
        ...accountIds.map((accountId, index) => assertion(write, key, `posting-account-${index}`, "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND status='active')", [accountId, actor.companyId, input.chartId])),
    ];
    const steps: SqlStatement[] = [...guards,
        write.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,is_opening,status,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(journalId, actor.companyId, input.chartId, input.periodId, number, input.entryDate, input.currency, fx, input.sourceType, input.sourceId ?? null, input.memo, input.isOpening ? 1 : 0, status, actor.userId, status === "posted" ? actor.userId : null, status === "posted" ? now : null, now, now),
        ...values.map((l, index) => write.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(`${journalId}-${index}`, journalId, l.accountId, l.description ?? "", l.debit, l.credit, l.baseDebit, l.baseCredit, JSON.stringify(l.dimensions ?? {}), index)),
    ];
    // Owner adapters consume these exact retained amounts; public journal results
    // and command receipts keep their existing shape.
    return { steps, guards, preparedLines: values, preparedFxEvidence, result: plan.result };
}
function openingService(provided?: SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined;
    const db = (actor: WorkContext, readOnly = false) => pending ??= financeOwnedDatabase(actor, provided, readOnly);
    return createOpeningService({ error: (...args) => new ApiError(...args), authorize: requirePermission, now: Date.now, digest: hash, ready: async (actor, readOnly) => { await db(actor, readOnly); }, period: async (actor, periodId, date) => ensurePeriod(await db(actor), actor, periodId, date), prepare: async (actor, input, key) => journalPreparation(await db(actor))(actor, input, key), replay: async (actor, key, operation, id, input) => replayCommand(await db(actor), actor, key, operation, id, input), replayOpening: async (actor, key, operation, id, input) => replayCommand(await db(actor), actor, key, operation, id, input), commitJournal: async ({ actor, input, id, journalId, plan }) => { const write = await db(actor), { steps, result } = journalStatements(write, actor, input, id, 'draft', plan); return executeCommand(write, actor, id, 'finance.journal.create', input, journalId, 1, result, steps); }, commitOpening: async ({ actor, commandId, journalId, commandInput, journalInput, definition, journal: plan, now }) => {
            const write = await db(actor), journal = journalStatements(write, actor, journalInput, commandId, 'draft', plan);
            const invoiceSteps = definition.subledgerItems.flatMap((item, index) => {
                const amount = decimalToMinor(item.amount), invoiceId = `${journalId}-subledger-${index}`, receivable = item.kind === "receivable";
                return [
                    assertion(write, commandId, `opening-party-${index}`, "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind=? AND currency=? AND status='active')", [item.partyId, actor.companyId, receivable ? "customer" : "supplier", definition.currency]),
                    assertion(write, commandId, `opening-control-${index}`, "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND status='active' AND is_control=1)", [item.controlAccountId, actor.companyId, definition.chartId]),
                    assertion(write, commandId, `opening-reference-${index}`, "NOT EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=? AND kind=? AND lower(invoice_number)=lower(?))", [actor.companyId, item.kind, item.reference]),
                    write.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,status,journal_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,0,?,?,?,?,?,?,?,?,'draft',?,?,?,?)").bind(invoiceId, actor.companyId, item.partyId, item.kind, item.reference, definition.entryDate, item.dueDate, definition.currency, amount, amount, amount, amount, amount, receivable ? item.controlAccountId : item.offsetAccountId, receivable ? item.offsetAccountId : item.controlAccountId, "opening_import", journalId, journalId, actor.userId, now, now),
                ];
            });
            const result = { ...journal.result, subledgerCount: definition.subledgerItems.length };
            return executeCommand(write, actor, commandId, "finance.journal.create", commandInput, journalId, 1, result, [...journal.steps, ...invoiceSteps]);
        }, openingChart: async (facts) => { const { input, actor } = facts; const write = await db(actor); return write.prepare("SELECT id,base_currency,updated_at FROM finance_charts WHERE id=? AND company_id=? AND status='active'").bind(input.chartId, actor.companyId).first<FinanceRow>(); },
        openingAccounts: async (facts) => { const { accountIds, actor, input } = facts; const write = await db(actor); return (await write.prepare(`SELECT id,code,name,status,is_control,control_kind,updated_at FROM finance_accounts WHERE company_id=? AND chart_id=? AND id IN (${accountIds.map(() => "?").join(",")})`).bind(actor.companyId, input.chartId, ...accountIds).all<FinanceRow>()).results; },
        openingParties: async (facts) => { const { partyIds, actor } = facts; const write = await db(actor); return (await write.prepare(`SELECT id,kind,currency,status,version,updated_at FROM finance_parties WHERE company_id=? AND id IN (${partyIds.map(() => "?").join(",")})`).bind(actor.companyId, ...partyIds).all<FinanceRow>()).results; },
        openingReference: async (facts) => { const { references, actor } = facts; const write = await db(actor); return write.prepare(`SELECT kind,invoice_number FROM finance_invoices WHERE company_id=? AND (${[...references].map(() => "(kind=? AND lower(invoice_number)=?)").join(" OR ")}) LIMIT 1`).bind(actor.companyId, ...[...references].flatMap(value => { const split = value.indexOf(":"); return [value.slice(0, split), value.slice(split + 1)]; })).first<FinanceRow>(); },
        openingExisting: async (facts) => { const { actor, input } = facts; const write = await db(actor); return !!await write.prepare("SELECT id FROM finance_journals WHERE company_id=? AND chart_id=? AND is_opening=1 AND status IN ('draft','in_review','posted') LIMIT 1").bind(actor.companyId, input.chartId).first(); } });
}
function accountImportService(provided?: SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined;
    const db = (actor: WorkContext) => pending ??= financeOwnedDatabase(actor, provided);
    const unused = "NOT EXISTS(SELECT 1 FROM finance_journal_lines WHERE account_id=?) AND NOT EXISTS(SELECT 1 FROM finance_invoices WHERE debit_account_id=? OR credit_account_id=? OR tax_account_id=?) AND NOT EXISTS(SELECT 1 FROM finance_invoice_line_versions WHERE account_id=? OR tax_account_id=?)";
    return createAccountImport({ error: (...args) => new ApiError(...args), authorize: requirePermission, ready: async (actor) => { await db(actor); }, now: Date.now, id: () => crypto.randomUUID(), prior: async (actor, key, input) => priorCommand(await db(actor), actor, key, 'finance.account.import', input), chart: async (actor, chartId) => (await db(actor)).prepare('SELECT base_currency FROM finance_charts WHERE id=? AND company_id=?').bind(chartId, actor.companyId).first<{
            base_currency: string;
        }>(), existing: async (actor, chartId, code) => (await db(actor)).prepare('SELECT * FROM finance_accounts WHERE chart_id=? AND company_id=? AND code=?').bind(chartId, actor.companyId, code).first<FinanceRow>(), unused: async (id) => !!await (await pending!).prepare(`SELECT (${unused}) unused`).bind(id, id, id, id, id, id).first('unused'), commit: async ({ actor, key, input, plans, now }) => {
            const write = await db(actor), chartId = input.chartId, pendingStatements: SqlStatement[] = [];
            for (const { id, index, row, existing, currency, controlKind, structuralChange } of plans) {
                if (structuralChange && existing) {
                    pendingStatements.push(assertion(write, key, `account-history-${index}`, unused, [existing.id, existing.id, existing.id, existing.id, existing.id, existing.id]));
                }
                if (existing)
                    pendingStatements.push(assertion(write, key, `account-version-${index}`, "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND version=?)", [existing.id, actor.companyId, existing.version]));
                else
                    pendingStatements.push(assertion(write, key, `new-account-${index}`, "NOT EXISTS(SELECT 1 FROM finance_accounts WHERE chart_id=? AND code=?)", [chartId, row.code]));
                pendingStatements.push(write.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(chart_id,code) DO UPDATE SET name=excluded.name,category=excluded.category,currency=excluded.currency,is_control=excluded.is_control,control_kind=excluded.control_kind,version=finance_accounts.version+1,updated_at=excluded.updated_at")
                    .bind(id, chartId, actor.companyId, row.code, row.name, row.category, currency, row.controlKind === undefined ? Number(existing?.is_control ?? (controlKind !== "none" ? 1 : 0)) : controlKind !== "none" ? 1 : 0, controlKind, now, now));
            }
            return saveCommand(write, actor, key, 'finance.account.import', input, { imported: input.rows.length, chartId }, pendingStatements);
        } });
}
async function journalPostStatements(write: SqlDatabase, actor: WorkContext, journal: FinanceRow, id: string, input: {
    expectedVersion: number;
}, plan: JournalPostPlan) {
    const journalId = plan.journalId;
    const snapshot = await legacyMoneySnapshot(write, actor, { sourceType: "finance_journal", sourceId: journalId, amountKind: "debit_total", transactionDate: String(journal.entry_date), currency: String(journal.currency), amountUnits: plan.debit });
    const { now, opening } = plan;
    return [
        assertion(write, id, "journal", "EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND company_id=? AND version=? AND status IN ('draft','in_review'))", [journalId, actor.companyId, input.expectedVersion]),
        assertion(write, id, "period", "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)", [journal.period_id, actor.companyId, journal.entry_date, journal.entry_date]),
        assertion(write, id, "active-accounts", "NOT EXISTS(SELECT 1 FROM finance_journal_lines l LEFT JOIN finance_accounts a ON a.id=l.account_id AND a.company_id=? AND a.chart_id=? LEFT JOIN finance_charts c ON c.id=a.chart_id WHERE l.journal_id=? AND (a.id IS NULL OR a.status!='active' OR a.currency NOT IN (?,c.base_currency)))", [actor.companyId, journal.chart_id, journalId, journal.currency]),
        assertion(write, id, "balanced", "(SELECT COUNT(*)>=2 AND SUM(base_debit_minor)=SUM(base_credit_minor) AND SUM(debit_minor)=SUM(credit_minor) FROM finance_journal_lines WHERE journal_id=?)", [journalId]),
        ...(opening ? [assertion(write, id, "opening-subledgers", "NOT EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=? AND source_type='opening_import' AND source_id=? AND (status!='draft' OR journal_id!=?))", [actor.companyId, journalId, journalId])] : []),
        write.prepare("UPDATE finance_journals SET status='posted',version=version+1,posted_by=?,posted_at=?,updated_at=? WHERE id=?").bind(actor.userId, now, now, journalId),
        ...(snapshot ? [snapshot] : []),
        ...(opening ? [write.prepare("UPDATE finance_invoices SET status='posted',approved_by=?,posted_at=?,version=version+1,updated_at=? WHERE company_id=? AND source_type='opening_import' AND source_id=? AND status='draft'").bind(actor.userId, now, now, actor.companyId, journalId)] : []),
    ];
}
function journalReviewService(provided?: SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined = provided ? Promise.resolve(provided) : undefined;
    const db = (actor: WorkContext) => pending ??= resources(actor).then(r => r.write);
    return createJournalReview({ error: (...args) => new ApiError(...args), authorize: requirePermission, now: Date.now, ready: async (actor) => { await db(actor); }, replay: async (actor, key, operation, id, input) => replayCommand(await db(actor), actor, key, operation, id, input), journal: async (actor, id) => (await db(actor)).prepare('SELECT * FROM finance_journals WHERE id=? AND company_id=?').bind(id, actor.companyId).first<FinanceRow>(), postedJournal: async (actor, id) => (await db(actor)).prepare("SELECT * FROM finance_journals WHERE id=? AND company_id=? AND status='posted'").bind(id, actor.companyId).first<FinanceRow>(), debit: async (journalId) => (await pending!).prepare('SELECT COALESCE(SUM(debit_minor),0) AS amount FROM finance_journal_lines WHERE journal_id=?').bind(journalId).first<{
            amount: number;
        }>(), openingAllocated: async (actor, journalId) => !!await (await db(actor)).prepare("SELECT 1 FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE i.company_id=? AND i.source_type='opening_import' AND i.source_id=?").bind(actor.companyId, journalId).first(), commitPost: async ({ actor, journalId, id, input, journal, plan }) => { const write = await db(actor); return executeCommand(write, actor, id, 'finance.journal.post', input, journalId, input.expectedVersion + 1, { id: journalId, status: 'posted', version: input.expectedVersion + 1 }, await journalPostStatements(write, actor, journal, id, input, plan)); }, sourceOpening: async (id) => !!await (await pending!).prepare("SELECT 1 FROM finance_journals WHERE id=? AND source_type='opening_import'").bind(id).first(), reversalRoot: async (actor, journalId) => (await db(actor)).prepare("WITH RECURSIVE lineage AS (SELECT id,source_type,source_id,0 depth FROM finance_journals WHERE id=? AND company_id=? UNION ALL SELECT p.id,p.source_type,p.source_id,l.depth+1 FROM finance_journals p JOIN lineage l ON p.id=l.source_id AND l.source_type='reversal' WHERE p.company_id=? AND l.depth<100) SELECT source_type FROM lineage ORDER BY depth DESC LIMIT 1").bind(journalId, actor.companyId, actor.companyId).first<{
            source_type: string;
        }>(), commitReverse: async ({ actor, journalId, id, input, journal, now, opening }) => {
            const write = await db(actor);
            const reversal = `reversal-${id}`;
            const money = await prepareJournalReversalMoney(write, actor, {journalId,reversalId:reversal,key:id,periodId:input.periodId,entryDate:input.entryDate,memo:input.memo ?? `Reversal of ${journal.journal_number}`,at:now});
            return executeCommand(write, actor, id, "finance.journal.reverse", input, journalId, Number(journal.version) + 1, { reversalJournalId: reversal, reversedJournalId: journalId }, [
                assertion(write, id, "source", "EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND version=? AND status='posted')", [journalId, journal.version]),
                ...(opening ? [assertion(write, id, "opening-unsettled", "NOT EXISTS(SELECT 1 FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE i.company_id=? AND i.source_type='opening_import' AND i.source_id=?)", [actor.companyId, journalId])] : []),
                assertion(write, id, "period", "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)", [input.periodId, actor.companyId, input.entryDate, input.entryDate]),
                write.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'reversal',id,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?").bind(reversal, input.periodId, `REV-${id}`, input.entryDate, input.memo ?? `Reversal of ${journal.journal_number}`, actor.userId, actor.userId, now, now, now, journalId),
                write.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?").bind(reversal, reversal, journalId),
                write.prepare("UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?").bind(actor.userId, now, now, journalId),
                money,
                ...(opening ? [write.prepare("UPDATE finance_invoices SET status='voided',outstanding_minor=0,base_outstanding_minor=0,version=version+1,updated_at=? WHERE company_id=? AND source_type='opening_import' AND source_id=?").bind(now, actor.companyId, journalId)] : []),
            ]);
        } });
}
function financeReadService(provided?:SqlDatabase,historyOnly=false) {
    let pending: Promise<SqlDatabase> | undefined;
    const db = (actor: WorkContext) => pending ??= provided ? (createFinanceResourcePolicy({error:(...args)=>new ApiError(...args),hasPermission})(actor,true),Promise.resolve(provided)) : resources(actor, true).then(r => r.write);
    return createFinanceReads({ error: (...args) => new ApiError(...args), authorize: requirePermission, ready: async (actor) => { await db(actor); }, currentLines: async (companyId, id, revision) => salesInvoiceLines(await pending!, companyId, id, revision), reportCharts: async (actor) => (await (await db(actor)).prepare('SELECT id FROM finance_charts WHERE company_id=?').bind(actor.companyId).all<{
            id: string;
        }>()).results, reportChart: async (actor, id) => !!await (await db(actor)).prepare('SELECT 1 FROM finance_charts WHERE id=? AND company_id=?').bind(id, actor.companyId).first(), reportPeriod: async (actor, id) => !!await (await db(actor)).prepare('SELECT 1 FROM finance_periods WHERE id=? AND company_id=?').bind(id, actor.companyId).first(), reportPeriodDates: async (actor, id) => (await db(actor)).prepare('SELECT start_date,end_date FROM finance_periods WHERE id=? AND company_id=?').bind(id, actor.companyId).first<{
            start_date: string;
            end_date: string;
        }>(), trialRows: async ({ actor, periodId, chartId, dates, options }) => {
            const write = await db(actor);
            const clauses = ["j.company_id=?", "j.status IN ('posted','reversed')"];
            const args: (string | number)[] = [actor.companyId];
            if (periodId) {
                clauses.push("j.period_id=?");
                args.push(periodId);
            }
            if (options.excludeYearClose)
                clauses.push(`NOT ${fiscalCloseSourceSql()}`);
            if (chartId) {
                clauses.push("j.chart_id=?");
                args.push(chartId);
            }
            if (dates.startDate) {
                clauses.push("j.entry_date>=?");
                args.push(dates.startDate);
            }
            if (dates.endDate) {
                clauses.push("j.entry_date<=?");
                args.push(dates.endDate);
            }
            const rows = await write.prepare(`SELECT a.id,a.code,a.name,a.category,COALESCE(SUM(l.base_debit_minor),0) AS debit,COALESCE(SUM(l.base_credit_minor),0) AS credit FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id JOIN finance_accounts a ON a.id=l.account_id WHERE ${clauses.join(" AND ")} GROUP BY a.id,a.code,a.name,a.category ORDER BY a.code`).bind(...args).all<FinanceRow>();
            return rows.results;
        }, charts: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_charts WHERE company_id=? ORDER BY name").bind(actor.companyId).all<FinanceRow>()).results; },
        accounts: async (facts) => { const { actor, chartId } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_accounts WHERE company_id=? AND chart_id=? ORDER BY code").bind(actor.companyId, chartId).all<FinanceRow>()).results; },
        invoices: async (facts) => {
            const { actor, limit } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT i.id,i.invoice_number,i.kind,i.invoice_date,i.due_date,i.currency,i.total_minor,i.outstanding_minor,i.status,p.name AS party_name
    FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id
    WHERE i.company_id=? ORDER BY i.invoice_date DESC,i.invoice_number DESC LIMIT ?`).bind(actor.companyId, Math.max(1, Math.min(limit, 100))).all<FinanceRow>()).results;
        },
        workInvoices: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT i.* FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.company_id=? ORDER BY i.updated_at DESC").bind(actor.companyId).all<FinanceRow>()).results; },
        invoice: async (facts) => {
            const { invoiceId, actor } = facts;
            const write = await db(actor);
            return write.prepare(`SELECT i.*,p.code AS party_code,p.name AS party_name,
    s.usd_units::text AS snapshot_usd_units,s.amount_units::text AS snapshot_source_units,s.fx_rate_id AS snapshot_rate_id
    FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id
    LEFT JOIN money_transaction_snapshots s ON s.company_id=i.company_id AND s.source_type='finance_invoice' AND s.source_id=i.id AND s.amount_kind='total'
    WHERE i.id=? AND i.company_id=?`).bind(invoiceId, actor.companyId).first<FinanceRow>();
        },
        invoiceAllocations: async (facts) => {
            const { invoiceId, actor } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT a.id,a.payment_id,a.amount_minor,a.created_at,p.payment_number,p.status,p.payment_slip_id
    FROM finance_allocations a JOIN finance_payments p ON p.id=a.payment_id
    WHERE a.invoice_id=? AND a.company_id=? ORDER BY a.created_at`).bind(invoiceId, actor.companyId).all<FinanceRow>()).results;
        },
        lineHistory: async (facts) => { const { invoiceId, actor } = facts; const write = await db(actor); return (await write.prepare("SELECT source_version,origin_kind,COUNT(*) line_count,SUM(subtotal_minor) subtotal_minor,SUM(tax_minor) tax_minor,MIN(created_at) created_at FROM finance_invoice_line_versions WHERE invoice_id=? AND company_id=? GROUP BY source_version,origin_kind ORDER BY source_version").bind(invoiceId, actor.companyId).all<FinanceRow>()).results; },
        thaiDocuments: async (facts) => { const { invoiceId, actor } = facts; const write = await db(actor); return (await write.prepare("SELECT id,document_number,document_kind,edition,issue_date FROM finance_issued_thai_documents WHERE invoice_id=? AND company_id=? ORDER BY edition").bind(invoiceId, actor.companyId).all<FinanceRow>()).results; },
        payments: async (facts) => {
            const { actor, limit } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT p.id,p.payment_number,p.kind,p.payment_date,p.currency,p.amount_minor,p.outstanding_minor,p.status,f.name AS party_name
    FROM finance_payments p LEFT JOIN finance_parties f ON f.id=p.party_id
    WHERE p.company_id=? ORDER BY p.payment_date DESC,p.payment_number DESC LIMIT ?`).bind(actor.companyId, Math.max(1, Math.min(limit, 100))).all<FinanceRow>()).results;
        },
        payment: async (facts) => {
            const { paymentId, actor } = facts;
            const write = await db(actor);
            return write.prepare(`SELECT p.*,f.code AS party_code,f.name AS party_name,
      (SELECT r.result_json::jsonb->>'settlementInvoiceId' FROM command_receipts r
        WHERE r.record_id=p.id AND r.operation='finance.payment.create' AND r.authority_json::jsonb->>'companyId'=p.company_id LIMIT 1) settlement_invoice_id
    FROM finance_payments p LEFT JOIN finance_parties f ON f.id=p.party_id
    WHERE p.id=? AND p.company_id=?`).bind(paymentId, actor.companyId).first<FinanceRow>();
        },
        paymentAllocations: async (facts) => {
            const { paymentId, actor } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT a.id,a.invoice_id,a.amount_minor,a.created_at,i.invoice_number,i.status
    FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id
    WHERE a.payment_id=? AND a.company_id=? ORDER BY a.created_at`).bind(paymentId, actor.companyId).all<FinanceRow>()).results;
        },
        paymentReversal: async (facts) => { const { paymentId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_payment_reversals WHERE payment_id=?").bind(paymentId).first<FinanceRow>(); },
        openPeriods: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC").bind(actor.companyId).all<FinanceRow>()).results; },
        confirmInvoices: async (facts) => { if(historyOnly)return [];
            const { actor, payment } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT id,invoice_number,kind,outstanding_minor,status,version
      FROM finance_invoices WHERE company_id=? AND party_id=? AND currency=? AND kind=? AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND source_type!='expense_claim' AND (?::text IS NULL OR id=?)
      ORDER BY due_date,invoice_number`).bind(actor.companyId, payment.party_id, payment.currency, payment.kind === "receipt" ? "receivable" : "payable", payment.settlement_invoice_id ?? null, payment.settlement_invoice_id ?? null).all<FinanceRow>()).results;
        },
        matchingBankRows: async (facts) => { if(historyOnly)return [];
            const { actor, payment } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT t.id,t.transaction_date,t.external_id,t.description,t.amount_minor,t.currency,t.match_status,t.version,s.statement_number
      FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id
      WHERE t.company_id=? AND s.bank_account=? AND t.currency=? AND t.match_status IN ('unmatched','proposed') AND t.amount_minor=? AND t.transaction_date>=?
      ORDER BY t.transaction_date DESC,t.external_id`).bind(actor.companyId, payment.bank_account, payment.currency, payment.kind === "receipt" ? payment.amount_minor : -(Number(payment.amount_minor) - Number(payment.withheld_minor ?? 0)), payment.payment_date).all<FinanceRow>()).results;
        },
        fxAccounts: async (facts) => { if(historyOnly)return [];
            const { payment, actor } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT a.id,a.code,a.name,a.category,a.currency,c.base_currency
      FROM finance_accounts bank JOIN finance_charts c ON c.id=bank.chart_id JOIN finance_accounts a ON a.chart_id=c.id
      WHERE bank.id=? AND bank.company_id=? AND a.company_id=bank.company_id AND a.status='active' AND a.currency=c.base_currency AND a.category IN ('other_income','other_expense')
      ORDER BY a.code`).bind(payment.bank_account, actor.companyId).all<FinanceRow>()).results;
        },
        workspaceAccounts: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code").bind(actor.companyId).all<FinanceRow>()).results; },
        workspaceParties: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_parties WHERE company_id=? AND status='active' AND kind IN ('customer','supplier') AND person_id IS NULL ORDER BY name").bind(actor.companyId).all<FinanceRow>()).results; },
        workspacePeriods: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC").bind(actor.companyId).all<FinanceRow>()).results; },
        statement: async (facts) => { const { id, actor } = facts; const write = await db(actor); return write.prepare("SELECT s.*,a.currency,a.code,a.name,c.base_currency FROM finance_bank_statements s JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=s.company_id JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=s.company_id WHERE s.id=? AND s.company_id=?").bind(id, actor.companyId).first<FinanceRow>(); },
        statementRows: async (facts) => { const { id, actor, page } = facts; const write = await db(actor); return (await write.prepare('SELECT * FROM finance_bank_transactions WHERE statement_id=? AND company_id=? ORDER BY transaction_date,id LIMIT 31 OFFSET ?').bind(id, actor.companyId, (page - 1) * 30).all<FinanceRow>()).results; },
        agingBalances: async (facts) => {
            const { actor, asOf, kind, chartId } = facts;
            const write = await db(actor);
            return (await write.prepare(`${invoiceHistoryCte}
    SELECT i.*,p.code party_code,p.name party_name FROM historical_invoice_balances i
    JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id
    WHERE i.kind=? AND i.historical_outstanding_minor>0 AND (?::text IS NULL OR i.chart_id=?) ORDER BY i.due_date,i.id`)
                .bind(actor.companyId, asOf, kind, chartId ?? null, chartId ?? null).all<HistoricalAgingRow>()).results;
        },
        listJournals: async (actor, query) => {
            const write = await db(actor);
            const clauses = ["company_id=?"];
            const args: (string | number)[] = [actor.companyId];
            if (query.periodId) {
                clauses.push("period_id=?");
                args.push(query.periodId);
            }
            if (query.status) {
                clauses.push("status=?");
                args.push(query.status);
            }
            return (await write.prepare(`SELECT * FROM finance_journals WHERE ${clauses.join(" AND ")} ORDER BY entry_date DESC,journal_number DESC LIMIT 500`).bind(...args).all<FinanceRow>()).results;
        },
        financeOverviewCounts: async (actor) => {
            const write = await db(actor);
            return (await write.prepare(`SELECT
    (SELECT count(*) FROM finance_journals WHERE company_id=? ) AS journals,
    (SELECT count(*) FROM finance_journals WHERE company_id=? AND status='draft') AS posting,
    (SELECT count(*) FROM finance_invoices WHERE company_id=?) AS invoices,
    (SELECT count(*) FROM finance_payments WHERE company_id=?) AS payments,
    (SELECT count(*) FROM finance_invoices WHERE company_id=? AND kind='receivable' AND status IN ('posted','partially_paid') AND outstanding_minor>0) AS receivable,
    (SELECT count(*) FROM finance_invoices WHERE company_id=? AND kind='payable' AND status IN ('posted','partially_paid') AND outstanding_minor>0) AS payable,
    (SELECT count(*) FROM finance_bank_transactions WHERE company_id=? AND match_status<>'matched') AS reconcile`)
                .bind(...Array(7).fill(actor.companyId)).first<{
                journals: number;
                posting: number;
                invoices: number;
                payments: number;
                receivable: number;
                payable: number;
                reconcile: number;
            }>())!;
        },
        invoicesPage: async (actor, query) => {
            const write = await db(actor);
            return readListPage<FinanceRow & {
                id: string;
                page_key: string;
            }>(write, {
                sql: `SELECT i.id,i.invoice_number,i.kind,i.invoice_date,i.due_date,i.currency,i.total_minor,i.outstanding_minor,i.status,p.name AS party_name,
      s.usd_units::text AS snapshot_usd_units,s.amount_units::text AS snapshot_source_units,s.fx_rate_id AS snapshot_rate_id,
      i.invoice_date||':'||i.invoice_number AS page_key FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id
      LEFT JOIN money_transaction_snapshots s ON s.company_id=i.company_id AND s.source_type='finance_invoice' AND s.source_id=i.id AND s.amount_kind='total'
      WHERE i.company_id=?
      AND (?='' OR EXISTS (SELECT 1 FROM finance_accounts a WHERE a.id=i.debit_account_id AND a.company_id=i.company_id AND a.chart_id=?))
      AND (?='' OR i.kind=?) AND (?='' OR i.status=?) AND (?<>'open' OR (i.status IN ('posted','partially_paid') AND i.outstanding_minor>0))
      AND (?='' OR strpos(lower(i.invoice_number||' '||p.name),lower(?))>0)`,
                args: [actor.companyId, query.chartId ?? '', query.chartId ?? '', query.kind ?? '', query.kind ?? '', query.status ?? '', query.status ?? '', query.queue ?? '', query.q ?? '', query.q ?? ''],
                descending: true, cursor: query.cursor, scope: [actor.companyId, 'invoices', query.chartId ?? '', query.q ?? '', query.kind ?? '', query.status ?? '', query.queue ?? ''],
            });
        },
        bankWorkspace: async (actor, query) => {
            const write = await db(actor);
            const statementSql = `SELECT s.*,a.currency,c.base_currency,s.statement_date AS page_key FROM finance_bank_statements s JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=s.company_id JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=s.company_id WHERE s.company_id=? AND (?='' OR a.chart_id=?)`;
            const transactionSql = `SELECT t.*,s.bank_account,s.statement_number,t.transaction_date AS page_key FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=t.company_id WHERE t.company_id=? AND (?='' OR a.chart_id=?) AND (?<>'unmatched' OR t.match_status<>'matched') AND (?='' OR strpos(lower(t.external_id||' '||COALESCE(t.description,'')),lower(?))>0)`;
            const statementArgs = [actor.companyId, query?.chart ?? '', query?.chart ?? ''];
            const transactionArgs = [...statementArgs, query?.view ?? '', query?.q ?? '', query?.q ?? ''];
            const paged = !!query && query.print !== '1';
            const [accounts, statements, transactions, payments, selectedTransaction] = await Promise.all([
                !query || query.new === '1' ? write.prepare("SELECT a.id,a.chart_id,a.code,a.name,a.currency,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.category='asset' AND a.control_kind IN ('none','bank') AND a.status='active' AND c.status='active' AND NOT EXISTS(SELECT 1 FROM finance_book_rules r WHERE r.company_id=a.company_id AND r.secondary_chart_id=a.chart_id AND r.status='approved') ORDER BY CASE WHEN a.control_kind='bank' THEN 0 ELSE 1 END,a.code,a.id").bind(actor.companyId).all<FinanceRow>() : { results: [] as FinanceRow[] },
                paged ? readListPage<FinanceRow & {
                    id: string;
                    page_key: string;
                }>(write, { sql: statementSql, args: statementArgs, cursor: query.statementCursor, descending: true, scope: [actor.companyId, 'bank-statements', query?.chart ?? ''] }).then(page => ({ results: page.items, page })) :
                    write.prepare(`${statementSql} ORDER BY s.statement_date DESC,s.id`).bind(...statementArgs).all<FinanceRow>().then(result => ({ ...result, page: null })),
                paged ? readListPage<FinanceRow & {
                    id: string;
                    page_key: string;
                }>(write, { sql: transactionSql, args: transactionArgs, cursor: query.transactionCursor, descending: true, scope: [actor.companyId, 'bank-transactions', query?.chart ?? '', query?.view ?? '', query?.q ?? ''] }).then(page => ({ results: page.items, page })) :
                    write.prepare(`${transactionSql} ORDER BY t.transaction_date DESC,t.id`).bind(...transactionArgs).all<FinanceRow>().then(result => ({ ...result, page: null })),
                !query || query.match ? write.prepare("SELECT p.* FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id WHERE p.company_id=? AND p.status IN ('released','settled') AND j.status='posted' AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t WHERE t.company_id=p.company_id AND t.matched_type='payment' AND t.matched_id=p.id AND t.match_status='matched') ORDER BY p.payment_date DESC").bind(actor.companyId).all<FinanceRow>() : { results: [] as FinanceRow[] },
                query?.match ? write.prepare("SELECT t.*,s.bank_account,s.statement_number FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id WHERE t.id=? AND t.company_id=?").bind(query.match, actor.companyId).first<FinanceRow>() : null,
            ]);
            return { accounts: accounts.results, statements: statements.results, statementPage: statements.page, transactions: transactions.results, transactionPage: transactions.page, payments: payments.results, selectedTransaction };
        } });
}
function journalBatchService(provided?: SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined;
    const db = (actor: WorkContext) => pending ??= financeOwnedDatabase(actor, provided);
    return createJournalBatch({ error: (...args) => new ApiError(...args), authorize: requirePermission, now: Date.now, digest: hash, ready: async (actor) => { await db(actor); }, replay: async (actor, key, operation, id, input) => replayCommand(await db(actor), actor, key, operation, id, input), journal: async (actor, id) => (await db(actor)).prepare('SELECT * FROM finance_journals WHERE id=? AND company_id=?').bind(id, actor.companyId).first<FinanceRow>(), lines: async (id) => (await (await pending!).prepare('SELECT id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal').bind(id).all<FinanceRow>()).results, preparePosting: async (actor, journal) => journalReviewService(await db(actor)).prepareJournalPost(actor, journal), commit: async ({ actor, input, id, batchId, plans, lineCount }) => {
            const write = await db(actor);
            const steps: SqlStatement[] = [], posted = [] as Array<{
                id: string;
                version: number;
            }>;
            for (const { item, childKey, journal, fingerprint, posting, evidenceDigest, now } of plans) {
                const flow = await prepareFlowCommand(write, actor, childKey, "journal", item.id, "post", { ...item, reason: input.reason }, item.expectedVersion + 1);
                steps.push(...flow.before, assertion(write, childKey, "batch-lines", "(SELECT jsonb_agg(jsonb_build_array(id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) ORDER BY ordinal) FROM finance_journal_lines WHERE journal_id=?)=?::jsonb", [item.id, fingerprint]), ...await journalPostStatements(write, actor, journal, childKey, item, posting), ...flow.after, write.prepare("INSERT INTO finance_batch_review_items(command_id,journal_id,company_id,reviewed_version,reviewed_by,assignment_id,evidence_digest,reason,reviewed_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(id, item.id, actor.companyId, item.expectedVersion, actor.userId, actor.assignmentId, evidenceDigest, input.reason, now), clearWorkflowCommandAssertions(write, childKey));
                posted.push({ id: item.id, version: item.expectedVersion + 1 });
            }
            return executeCommand(write, actor, id, "finance.batch.post", input, batchId, 1, { id: batchId, status: "posted", journals: posted, lineCount }, steps);
        } });
}
function invoiceService(provided?: SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined;
    const db = (actor: WorkContext) => pending ??= resources(actor).then(runtime => {
        if (provided) {
            const owner = runtime.write as SqlDatabase & {
                assertActiveTransaction?: (db: SqlDatabase) => void;
            };
            if (!owner.assertActiveTransaction)
                throw new ApiError(503, 'TRANSACTION_OWNERSHIP_REQUIRED', ownershipMessage);
            owner.assertActiveTransaction(provided);
        }
        return provided ?? runtime.write;
    });
    let ownershipMessage = 'Billing requires an owned PostgreSQL transaction.';
    let aiSteps: SqlStatement[] = [], lineRevision: Awaited<ReturnType<typeof prepareInvoiceLineRevision>> | undefined, attachmentBinding: Awaited<ReturnType<typeof prepareAttachmentBinding>> | null = null, native: Awaited<ReturnType<typeof nativeInvoicePosting>> | null = null, matched: Awaited<ReturnType<typeof import('@/server/modules/procurement/public/supplier-bill-matching').matchedInvoicePosting>> | null = null, journalNative: Awaited<ReturnType<typeof prepareJournal>> | undefined, goods: Awaited<ReturnType<typeof import('@/server/modules/commerce/public/commerce-goods-accounting').prepareCommerceInvoiceGoods>> | null = null, snapshot: SqlStatement | undefined;
    return createInvoiceService({ error: (...args) => new ApiError(...args), authorize: requirePermission, now: Date.now, ready: async (actor, action) => { ownershipMessage = action === 'post' ? 'Invoice posting requires an owned PostgreSQL transaction.' : 'Billing requires an owned PostgreSQL transaction.'; await db(actor); }, moneyScale: async (companyId, currency) => requireFinanceMoneyScale(await pending!, companyId, currency), replay: async <Result>(actor:WorkContext,key:string,operation:string,id:string,input:unknown) => replayCommand<Result>(await db(actor), actor, key, operation, id, input), invoiceAccounts: async (actor, currency, ids) => (await (await db(actor)).prepare(`SELECT id,chart_id FROM finance_accounts WHERE company_id=? AND status='active' AND currency=? AND id IN (${ids.map(() => '?').join(',')})`).bind(actor.companyId, currency, ...ids).all<{
            id: string;
            chart_id: string;
        }>()).results, prepareAiDraft: async (actor, proposalId, invoiceId, input, key) => { aiSteps = await (await import('@/server/modules/finance/public/finance-ai')).financeAiDraftApplication(await db(actor), actor, await (await import('@/server/modules/runtime/public/runtime')).runtimeEnv(), proposalId, invoiceId, input, key); }, businessBilled: async (actor, id) => !!await (await db(actor)).prepare('SELECT 1 FROM finance_business_billing_runs WHERE invoice_id=? AND company_id=?').bind(id, actor.companyId).first(), editableInvoice: async (actor, id, version) => (await db(actor)).prepare("SELECT line_mode FROM finance_invoices WHERE id=? AND company_id=? AND status='draft' AND version=? AND source_type='standalone'").bind(id, actor.companyId, version).first<{
            line_mode: string;
        }>(), prepareLines: async (actor, input, id, revision, amounts, key) => { lineRevision = await prepareInvoiceLineRevision(await db(actor), actor, input, id, revision, amounts, key); return { mode: lineRevision.mode, revision: lineRevision.revision, count: lineRevision.count }; }, postableInvoice: async (actor, id) => (await db(actor)).prepare("SELECT * FROM finance_invoices WHERE id=? AND company_id=? AND status='draft' AND source_type!='opening_import'").bind(id, actor.companyId).first<FinanceRow>(), invoiceChart: async (actor, accountId) => (await db(actor)).prepare('SELECT a.chart_id,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=?').bind(accountId, actor.companyId).first<{
            chart_id: string;
            base_currency: string;
        }>(), prepareReviewAttachments: async (actor, invoiceId, version, attachmentSetIds, id) => { attachmentBinding = await prepareAttachmentBinding(await db(actor), actor, { attachmentSetIds, bindingContract: { targetType: 'supplier_bill', bindingType: 'supplier_bill_posting', contexts: [{ purposeKey: 'bill_match_support', componentKind: 'action', componentRef: 'invoice-review' }] }, targetId: invoiceId, expectedTargetVersion: version, commandId: id, bindingId: `${invoiceId}:${version + 1}` }); }, nativeLines: async (actor, invoice, id) => { native = await nativeInvoicePosting(await db(actor), actor, invoice, id); return native ? { lines: native.lines } : null; }, matchedLines: async (actor, invoice, id) => { matched = await (await import('@/server/modules/procurement/public/supplier-bill-matching')).matchedInvoicePosting(await db(actor), actor, invoice, id); return matched ? { lines: matched.lines } : null; }, prepareJournal: async (actor, input, key, status) => { const write = await db(actor), plan = await journalPreparation(write)(actor, input, key, status); journalNative = journalStatements(write, actor, input, key, status, plan); return plan; }, prepareGoods: async (actor, { invoice, key, periodId, entryDate, gainAccountId, lossAccountId }) => { goods = await (await import('@/server/modules/commerce/public/commerce-goods-accounting')).prepareCommerceInvoiceGoods(await db(actor), actor, { invoice, journal: journalNative!, key, periodId, entryDate, gainAccountId, lossAccountId }); }, preparePostedMoney: async (actor, invoiceId, invoice, total) => {
            const write = await db(actor);
            const draftSnapshot = await write.prepare("SELECT 1 FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_invoice' AND source_id=? AND amount_kind=?").bind(actor.companyId, invoiceId, `draft:v${invoice.line_revision}`).first();
            const original=draftSnapshot?{sourceType:"finance_invoice",sourceId:invoiceId,amountKind:`draft:v${invoice.line_revision}`} : await fieldServiceInvoiceMoneySource(write,actor.companyId,invoiceId);
            snapshot = original ? (await snapshotMoneyCarry(write, actor, { sourceType: "finance_invoice", sourceId: invoiceId, amountKind: "total", transactionDate: String(invoice.invoice_date), amountUnits: String(total), original })).statement : await legacyMoneySnapshot(write, actor, { sourceType: "finance_invoice", sourceId: invoiceId, amountKind: "total", transactionDate: String(invoice.invoice_date), currency: String(invoice.currency), amountUnits: total });
        }, commitDraft: async ({ actor, input, id, existingId, invoiceId, operation, subtotal, tax, total, revision, now }) => {
            const write = await db(actor);
            const revisionNative = lineRevision!;
            return executeCommand(write, actor, id, operation, input, invoiceId, existingId ? (input.expectedVersion ?? 0) + 1 : 1, { id: invoiceId, total: minorToDecimal(total), outstanding: minorToDecimal(total), status: "draft", version: existingId ? (input.expectedVersion ?? 0) + 1 : 1 }, [
                ...aiSteps,
                ...revisionNative.guards,
                ...(existingId ? [assertion(write, id, "invoice-version", "EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND status='draft' AND version=? AND source_type='standalone')", [existingId, actor.companyId, input.expectedVersion ?? 0])] : []),
                assertion(write, id, "party", "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind=? AND currency=? AND status='active' AND person_id IS NULL)", [input.partyId!, actor.companyId, input.kind === "payable" ? "supplier" : "customer", input.currency]),
                existingId ? write.prepare("UPDATE finance_invoices SET party_id=?,kind=?,invoice_number=?,invoice_date=?,due_date=?,currency=?,subtotal_minor=?,tax_minor=?,total_minor=?,outstanding_minor=?,tax_code=?,debit_account_id=?,credit_account_id=?,tax_account_id=?,created_by=?,version=version+1,updated_at=? WHERE id=?").bind(input.partyId, input.kind, input.invoiceNumber, input.invoiceDate, input.dueDate, input.currency, subtotal, tax, total, total, input.taxCode ?? null, input.debitAccountId, input.creditAccountId, input.taxAccountId ?? null, actor.userId, now, existingId) : write.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,tax_code,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(invoiceId, actor.companyId, input.partyId, input.kind, input.invoiceNumber, input.invoiceDate, input.dueDate, input.currency, subtotal, tax, total, total, input.taxCode ?? null, input.debitAccountId, input.creditAccountId, input.taxAccountId ?? null, "standalone", null, actor.userId, now, now),
                write.prepare("UPDATE finance_invoices SET line_mode=?,line_revision=? WHERE id=? AND company_id=?").bind(revisionNative.mode, revision, invoiceId, actor.companyId),
                ...revisionNative.steps,
            ]);
        }, commitPost: async ({ actor, invoiceId, input, id, invoice, baseTotal, now }) => {
            const write = await db(actor), journal = journalNative!;
            return executeCommand(write, actor, id, "finance.invoice.post", input, invoiceId, Number(invoice.version) + 1, { id: invoiceId, status: "posted", journalId: journal.result.id }, [
                assertion(write, id, "invoice", "EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND status='draft' AND version=?)", [invoiceId, actor.companyId, invoice.version]), ...(attachmentBinding?.guards ?? []), ...(native?.guards ?? []), ...journal.steps,
                write.prepare("UPDATE finance_invoices SET status='posted',journal_id=?,fx_rate_micros=?,base_total_minor=?,base_outstanding_minor=?,approved_by=?,posted_at=?,version=version+1,updated_at=? WHERE id=?").bind(journal.result.id, journal.result.fxRateMicros, baseTotal, baseTotal, actor.userId, now, now, invoiceId),
                ...(snapshot ? [snapshot] : []),
                ...(goods?.steps ?? []), ...(matched?.steps ?? []),
                ...(attachmentBinding?.steps ?? []),
            ]);
        }, commitVoid: async ({ actor, invoiceId, expectedVersion, key, now }) => {
            const write = await db(actor);
            return executeCommand(write, actor, key, "finance.invoice.void", { expectedVersion }, invoiceId, expectedVersion + 1, { id: invoiceId, status: "voided", version: expectedVersion + 1 }, [
                assertion(write, key, "draft", "EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND status='draft' AND source_type='standalone' AND version=?)", [invoiceId, actor.companyId, expectedVersion]),
                write.prepare("UPDATE finance_invoices SET status='voided',outstanding_minor=0,version=version+1,updated_at=? WHERE id=?").bind(now, invoiceId),
            ]);
        } });
}
function paymentService(provided?: SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined;
    const db = (actor: WorkContext) => pending ??= resources(actor).then(r => provided ?? r.write);
    let requestedSnapshot: SqlStatement | undefined, withholding: Awaited<ReturnType<typeof readyPaymentWithholding>> | null = null, attachmentBinding: Awaited<ReturnType<typeof prepareAttachmentBinding>> | null = null, snapshot: SqlStatement | undefined, bankSnapshot: SqlStatement | undefined, withheldSnapshot: SqlStatement | null = null, taxConsumption: Awaited<ReturnType<typeof prepareWithholdingConsumption>> | null = null;
    const journals = new Map<string, Awaited<ReturnType<typeof prepareJournal>>>();
    return createFinancePayments({ settlementInvoice: async (actor,id) => (await db(actor)).prepare("SELECT * FROM finance_invoices WHERE id=? AND company_id=? FOR SHARE").bind(id,actor.companyId).first<FinanceRow>(), error: (...args) => new ApiError(...args), authorize: requirePermission, now: Date.now, id: () => crypto.randomUUID(), digest: hash, ready: async (actor) => { await db(actor); }, prior: async <Result>(actor:WorkContext,key:string,operation:string,input:unknown) => priorCommand<Result>(await db(actor), actor, key, operation, input), replay: async <Result>(actor:WorkContext,key:string,operation:string,id:string,input:unknown) => replayCommand<Result>(await db(actor), actor, key, operation, id, input), moneyScale: async (companyId, currency) => requireFinanceMoneyScale(await pending!, companyId, currency), prepareRequestedMoney: async (actor, paymentId, input, currency, amount) => { requestedSnapshot = await legacyMoneySnapshot(await db(actor), actor, { sourceType: 'finance_payment', sourceId: paymentId, amountKind: 'requested', transactionDate: input.paymentDate, currency, amountUnits: amount }); }, withholding: async (actor, payment, key, date, allocations) => { withholding = await readyPaymentWithholding(await db(actor), actor, payment, key, date, allocations); return withholding ? { source: { liabilityAccountId: withholding.source.liabilityAccountId } } : null; }, prepareAttachments: async (actor, paymentId, input, key) => { attachmentBinding = await prepareAttachmentBinding(await db(actor), actor, { attachmentSetIds: input.attachmentSetIds, bindingContract: { targetType: 'finance_payment', bindingType: 'finance_payment_confirmation', contexts: [{ purposeKey: 'payment_proof', componentKind: 'action', componentRef: 'bank-confirmation' }] }, targetId: paymentId, expectedTargetVersion: input.expectedVersion, commandId: key, bindingId: `${paymentId}:${input.expectedVersion + 1}` }); }, functionalFx: async (actor, currency, baseCurrency, date) => confirmedFunctionalFxMicros(await db(actor), actor, currency, baseCurrency, date), prepareJournal: async (actor, input, key, status) => { const write = await db(actor), plan = await journalPreparation(write)(actor, input, key, status); journals.set(plan.result.id, journalStatements(write, actor, input, key, status, plan)); return plan; }, prepareSettlementEvidence: async (actor, paymentId, payment, bank, journal, key, netBankMinor) => {
            const write = await db(actor);
            snapshot = await legacyMoneySnapshot(write, actor, { sourceType: "finance_payment", sourceId: paymentId, amountKind: "settled", transactionDate: String(bank.transaction_date), currency: String(payment.currency), amountUnits: Number(payment.amount_minor) });
            bankSnapshot = await legacyMoneySnapshot(write, actor, { sourceType: "finance_payment", sourceId: paymentId, amountKind: "bank_settled", transactionDate: String(bank.transaction_date), currency: String(payment.currency), amountUnits: netBankMinor });
            withheldSnapshot = withholding && Number(payment.withheld_minor) > 0 ? await legacyMoneySnapshot(write, actor, { sourceType: "finance_payment", sourceId: paymentId, amountKind: "withheld", transactionDate: String(bank.transaction_date), currency: "THB", amountUnits: Number(payment.withheld_minor) }) : null;
            taxConsumption = withholding ? await prepareWithholdingConsumption(write, actor, withholding, { commandId: key, paymentId, bankTransactionId: String(bank.id), bankDate: String(bank.transaction_date), journalId: journal.result.id }) : null;
        }, commitCreate: async ({ actor, input, id, paymentId, amount, currency, now, output }) => {
            const write = await db(actor), pendingStatements: SqlStatement[] = [];
            pendingStatements.push(...(input.settlementInvoiceId ? [assertion(write,id,"settlement-source","EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND party_id=? AND currency=? AND kind=? AND status IN ('posted','partially_paid') AND outstanding_minor>=? AND source_type<>'expense_claim')",[input.settlementInvoiceId,actor.companyId,input.partyId!,currency,input.kind==="receipt"?"receivable":"payable",amount])] : []), requestedSnapshot!, assertion(write, id, "party", "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind=? AND currency=? AND status='active' AND person_id IS NULL)", [input.partyId!, actor.companyId, input.kind === "payment" ? "supplier" : "customer", currency]), assertion(write, id, "bank-account", "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND category='asset' AND control_kind='bank' AND currency=? AND status='active')", [input.bankAccount!, actor.companyId, currency]), write.prepare("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(paymentId, actor.companyId, input.partyId, input.kind, input.paymentNumber, input.paymentDate, currency, amount, amount, input.bankAccount, actor.userId, now, now));
            return saveCommand(write, actor, id, "finance.payment.create", input, output, pendingStatements);
        }, commitApprove: async ({ actor, paymentId, input, id, now, output }) => { const write = await db(actor), pendingStatements: SqlStatement[] = []; pendingStatements.push(...(withholding?.guards ?? [])); pendingStatements.push(assertion(write, id, "payment-version", "EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND version=? AND status IN ('draft','in_review') AND created_by!=?)", [paymentId, actor.companyId, input.expectedVersion, actor.userId])); pendingStatements.push(write.prepare("UPDATE finance_payments SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=? AND version=? AND company_id=?").bind(actor.userId, now, paymentId, input.expectedVersion, actor.companyId), financeOwnerDecisionEffects(write).appendReviewedDecision({id:crypto.randomUUID(),commandId:id,recordId:paymentId,action:"finance.payment.approve",actorId:actor.userId,assignmentId:actor.assignmentId,reviewedVersion:input.expectedVersion,inputDigest:await hash(input),createdAt:now})); return saveCommand(write, actor, id, "finance.payment.approve", input, output, pendingStatements); }, commitRelease: async ({ actor, paymentId, input, id, now, output }) => { const write = await db(actor), pendingStatements: SqlStatement[] = []; pendingStatements.push(...(withholding?.guards ?? [])); pendingStatements.push(assertion(write, id, "payment-version", "EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND version=? AND status='approved' AND created_by!=? AND approved_by!=?)", [paymentId, actor.companyId, input.expectedVersion, actor.userId, actor.userId])); pendingStatements.push(write.prepare("UPDATE finance_payments SET status='released',released_by=?,version=version+1,updated_at=? WHERE id=? AND version=? AND company_id=?").bind(actor.userId, now, paymentId, input.expectedVersion, actor.companyId), financeOwnerDecisionEffects(write).appendReviewedDecision({id:crypto.randomUUID(),commandId:id,recordId:paymentId,action:"finance.payment.release",actorId:actor.userId,assignmentId:actor.assignmentId,reviewedVersion:input.expectedVersion,inputDigest:await hash(input),createdAt:now})); return saveCommand(write, actor, id, "finance.payment.release", input, output, pendingStatements); }, commitConfirm: async ({ actor, paymentId, input, key, payment, bank, signed, byId, amounts, baseAllocations, settlementRate, basePayment, journal: plan, fxJournal: fxPlan, gain, loss, netBankMinor, now }) => {
            const write = await db(actor), journal = journals.get(plan.result.id)!, fxJournal = fxPlan ? journals.get(fxPlan.result.id)! : null;
            return executeCommand(write, actor, key, "finance.payment.confirm", input, paymentId, input.expectedVersion + 1, { id: paymentId, status: "settled", version: input.expectedVersion + 1, journalId: journal.result.id, fxJournalId: fxJournal?.result.id ?? null, realizedFx: minorToDecimal(gain - loss), bankTransactionId: bank.id, withheldMinor: Number(payment.withheld_minor ?? 0), netBankMinor, withholdingDocumentId: taxConsumption?.documentId ?? null }, [
                ...(attachmentBinding?.guards ?? []), ...(withholding?.guards ?? []),
                assertion(write, key, "payment", "EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND version=? AND status='released' AND released_by=? AND created_by!=? AND approved_by!=?)", [paymentId, actor.companyId, input.expectedVersion, actor.userId, actor.userId, actor.userId]),
                assertion(write, key, "bank", "EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND s.bank_account=? AND t.currency=? AND t.amount_minor=? AND t.transaction_date>=?)", [bank.id, actor.companyId, input.bankVersion, payment.bank_account, payment.currency, signed, payment.payment_date]),
                ...input.allocations.map((item, index) => assertion(write, key, `invoice-${index}`, "EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND version=? AND status IN ('posted','partially_paid') AND outstanding_minor>=?)", [item.invoiceId, actor.companyId, byId.get(item.invoiceId)!.version, amounts[index]])),
                ...journal.steps,
                ...(fxJournal?.steps ?? []),
                ...baseAllocations.map((item, index) => write.prepare("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,realized_fx_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(`allocation-${key}-${index}`, actor.companyId, paymentId, item.invoiceId, item.amount, item.baseInvoice, item.baseSettlement, item.difference, actor.userId, now)),
                ...baseAllocations.map(item => write.prepare("UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,base_outstanding_minor=base_outstanding_minor-?,status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?").bind(item.amount, item.baseInvoice, item.amount, now, item.invoiceId)),
                write.prepare("UPDATE finance_payments SET outstanding_minor=0,status='settled',journal_id=?,fx_rate_micros=?,base_amount_minor=?,fx_journal_id=?,version=version+1,updated_at=? WHERE id=?").bind(journal.result.id, settlementRate, basePayment, fxJournal?.result.id ?? null, now, paymentId),
                snapshot!, bankSnapshot!, ...(withheldSnapshot ? [withheldSnapshot] : []), ...(taxConsumption?.steps ?? []),
                write.prepare("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?").bind(paymentId, bank.id),
                ...(attachmentBinding?.steps ?? []),
                financeOwnerDecisionEffects(write).appendReviewedDecision({id:crypto.randomUUID(),commandId:key,recordId:paymentId,action:"finance.payment.confirm",actorId:actor.userId,assignmentId:actor.assignmentId,reviewedVersion:input.expectedVersion,inputDigest:await hash(input),createdAt:now}),
            ]);
        }, commitReverse: async ({ actor, paymentId, input, id, payment, allocations, now, reversal, fxReversal }) => {
            const write = await db(actor);
            const steps: SqlStatement[] = [
                assertion(write, id, "payment", "EXISTS(SELECT 1 FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id WHERE p.id=? AND p.company_id=? AND p.version=? AND p.status='settled' AND j.status='posted')", [paymentId, actor.companyId, input.expectedVersion]),
                assertion(write, id, "bank-match", "(SELECT COUNT(*) FROM finance_bank_transactions WHERE company_id=? AND matched_type='payment' AND matched_id=? AND match_status='matched')<=1", [actor.companyId, paymentId]),
                assertion(write, id, "period", "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)", [input.periodId, actor.companyId, input.entryDate, input.entryDate]),
                assertion(write, id, "not-reversed", "NOT EXISTS(SELECT 1 FROM finance_payment_reversals WHERE payment_id=?)", [paymentId]),
                assertion(write, id, "credited-payment-source", "NOT EXISTS(SELECT 1 FROM finance_billing_corrections c JOIN finance_allocations a ON a.invoice_id=c.invoice_id WHERE a.payment_id=? AND c.company_id=? AND c.status='posted')", [paymentId, actor.companyId]),
                write.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'payment_reversal',?,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?").bind(reversal, input.periodId, `PAY-REV-${id}`, input.entryDate, paymentId, input.reason, actor.userId, actor.userId, now, now, now, payment.journal_id),
                write.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?").bind(reversal, reversal, payment.journal_id),
                write.prepare("UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?").bind(actor.userId, now, now, payment.journal_id),
                ...(fxReversal ? [
                    assertion(write, id, "fx-journal", "EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND company_id=? AND status='posted')", [payment.fx_journal_id, actor.companyId]),
                    write.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'payment_fx_reversal',?,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?").bind(fxReversal, input.periodId, `PAY-FX-REV-${id}`, input.entryDate, paymentId, input.reason, actor.userId, actor.userId, now, now, now, payment.fx_journal_id),
                    write.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?").bind(fxReversal, fxReversal, payment.fx_journal_id),
                    write.prepare("UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?").bind(actor.userId, now, now, payment.fx_journal_id),
                ] : []),
            ];
            for (const allocation of allocations) {
                steps.push(financeCloseCommerceSource(write).reversalGuard(id,`return-${allocation.id}`,allocation.invoice_id), assertion(write, id, `invoice-${allocation.id}`, "EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND outstanding_minor=? AND total_minor>=outstanding_minor+?)", [allocation.invoice_id, allocation.outstanding_minor, allocation.amount_minor]), write.prepare("UPDATE finance_invoices SET outstanding_minor=outstanding_minor+?,base_outstanding_minor=base_outstanding_minor+?,status=CASE WHEN outstanding_minor+?=total_minor THEN 'posted' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?").bind(allocation.amount_minor, allocation.base_invoice_minor, allocation.amount_minor, now, allocation.invoice_id));
            }
            const taxReversal = await prepareWithholdingReversal(write, actor, payment, id, reversal, input.reason);
            const originalMoney = await write.prepare("SELECT 1 AS present FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_payment' AND source_id=? AND amount_kind='settled'").bind(actor.companyId, paymentId).first<{
                present: number;
            }>();
            const supplierMoney=await prepareSupplierReversalMoney(write,actor,{key:id,paymentId,journalId:reversal,amount:Number(payment.amount_minor),currency:String(payment.currency),entryDate:input.entryDate,at:now});
            const moneyReversal = supplierMoney ? {statement:supplierMoney} : originalMoney ? await snapshotMoneyReversal(write, actor, { sourceType: "finance_payment_reversal", sourceId: reversal, amountKind: "gross", transactionDate: input.entryDate, original: { sourceType: "finance_payment", sourceId: paymentId, amountKind: "settled" }, amountUnits: String(payment.amount_minor) }) : null;
            const taxMoneyReversals: SqlStatement[] = [];
            if (payment.withholding_assessment_id) {
                for (const [amountKind, amount] of [["bank_settled", Number(payment.amount_minor) - Number(payment.withheld_minor)], ["withheld", Number(payment.withheld_minor)]] as const) {
                    const source = await write.prepare("SELECT 1 FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_payment' AND source_id=? AND amount_kind=?").bind(actor.companyId, paymentId, amountKind).first();
                    if (source && amount > 0)
                        taxMoneyReversals.push((await snapshotMoneyReversal(write, actor, { sourceType: "finance_payment_reversal", sourceId: reversal, amountKind, transactionDate: input.entryDate, original: { sourceType: "finance_payment", sourceId: paymentId, amountKind }, amountUnits: String(amount) })).statement);
                }
            }
            steps.push(...taxReversal, ...(moneyReversal ? [moneyReversal.statement] : []), ...taxMoneyReversals, write.prepare("UPDATE finance_bank_transactions SET match_status='unmatched',matched_type=NULL,matched_id=NULL,version=version+1 WHERE company_id=? AND matched_type='payment' AND matched_id=? AND match_status='matched'").bind(actor.companyId, paymentId), write.prepare("UPDATE finance_payments SET status='voided',version=version+1,updated_at=? WHERE id=?").bind(now, paymentId), write.prepare("INSERT INTO finance_payment_reversals(id,payment_id,reversal_journal_id,fx_reversal_journal_id,reason,reversed_by,reversed_at) VALUES(?,?,?,?,?,?,?)").bind(`payment-correction-${id}`, paymentId, reversal, fxReversal, input.reason, actor.userId, now));
            return executeCommand(write, actor, id, "finance.payment.reverse", input, paymentId, input.expectedVersion + 1, { id: paymentId, status: "voided", version: input.expectedVersion + 1, reversalJournalId: reversal }, steps);
        }, commitAllocation: async ({ actor, paymentId, input, key, amount, now }) => {
            const write = await db(actor);
            return executeCommand(write, actor, key, "finance.payment.allocate", input, paymentId, input.expectedVersion + 1, { paymentId, invoiceId: input.invoiceId, allocated: minorToDecimal(amount) }, [
                assertion(write, key, "supplier-reservation", "NOT EXISTS(SELECT 1 FROM finance_supplier_instructions WHERE invoice_id=? AND status IN ('reserved','exported','unknown'))", [input.invoiceId]),
                assertion(write, key, "targets", "EXISTS(SELECT 1 FROM finance_payments p JOIN finance_invoices i ON i.id=? JOIN finance_journals j ON j.id=p.journal_id WHERE p.id=? AND p.company_id=? AND i.company_id=p.company_id AND i.source_type!='expense_claim' AND p.party_id=i.party_id AND p.currency=i.currency AND p.version=? AND p.status IN ('released','settled') AND j.status='posted' AND i.status IN ('posted','partially_paid') AND ((p.kind='receipt' AND i.kind='receivable') OR (p.kind='payment' AND i.kind='payable')) AND p.outstanding_minor>=? AND i.outstanding_minor>=?)", [input.invoiceId, paymentId, actor.companyId, input.expectedVersion, amount, amount]),
                assertion(write, key, "allocation-history", "EXISTS(SELECT 1 FROM finance_payments p JOIN finance_journals pj ON pj.id=p.journal_id JOIN finance_periods period ON period.id=pj.period_id JOIN finance_charts c ON c.id=pj.chart_id JOIN finance_invoices i ON i.id=? JOIN finance_journals ij ON ij.id=i.journal_id WHERE p.id=? AND p.company_id=? AND period.company_id=p.company_id AND period.state='open' AND pj.entry_date>=ij.entry_date AND pj.chart_id=ij.chart_id AND p.currency=c.base_currency AND i.fx_rate_micros=1000000 AND COALESCE(p.fx_rate_micros,1000000)=1000000 AND EXISTS(SELECT 1 FROM finance_journal_lines l WHERE l.journal_id=pj.id AND l.account_id=CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END))", [input.invoiceId, paymentId, actor.companyId]),
                write.prepare("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(`allocation-${key}`, actor.companyId, paymentId, input.invoiceId, amount, amount, amount, actor.userId, now),
                write.prepare("UPDATE finance_payments SET outstanding_minor=outstanding_minor-?,status=CASE WHEN outstanding_minor=? THEN 'settled' ELSE status END,version=version+1,updated_at=? WHERE id=?").bind(amount, amount, now, paymentId),
                write.prepare("UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,base_outstanding_minor=GREATEST(0,base_outstanding_minor-?),status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?").bind(amount, amount, amount, now, input.invoiceId),
            ]);
        }, commitExport: async ({ actor, id, input, output }) => saveCommand(await db(actor), actor, id, 'finance.payment.export', input, output, []), reviewPayment: async (facts) => { const { paymentId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_payments WHERE id=? AND company_id=?").bind(paymentId, actor.companyId).first<FinanceRow>(); },
        releasedApprovedPayment: async (facts) => { const { paymentId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_payments WHERE id=? AND company_id=? AND status='approved'").bind(paymentId, actor.companyId).first<FinanceRow>(); },
        confirmationPayment: async (facts) => { const { paymentId, actor } = facts; const write = await db(actor); return write.prepare("SELECT p.*,(SELECT r.result_json::jsonb->>'settlementInvoiceId' FROM command_receipts r WHERE r.record_id=p.id AND r.operation='finance.payment.create' AND r.authority_json::jsonb->>'companyId'=p.company_id ORDER BY r.created_at LIMIT 1) settlement_invoice_id FROM finance_payments p WHERE p.id=? AND p.company_id=?").bind(paymentId, actor.companyId).first<FinanceRow>(); },
        allocationInvoices: async (facts) => { const { ids, actor } = facts; const write = await db(actor); return (await write.prepare(`SELECT * FROM finance_invoices WHERE company_id=? AND id IN (${ids.map(() => "?").join(",")})`).bind(actor.companyId, ...ids).all<FinanceRow>()).results; },
        confirmationBank: async (facts) => { const { input, actor } = facts; const write = await db(actor); return write.prepare("SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=?").bind(input.bankTransactionId, actor.companyId).first<FinanceRow>(); },
        confirmationBankAccount: async (facts) => { const { payment, actor } = facts; const write = await db(actor); return write.prepare("SELECT a.id,a.chart_id,a.currency,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=? AND a.category='asset' AND a.control_kind='bank' AND a.status='active'").bind(payment.bank_account, actor.companyId).first<FinanceRow>(); },
        confirmationAccounts: async (facts) => { const { accountIds, actor } = facts; const write = await db(actor); return (await write.prepare(`SELECT id,chart_id,currency FROM finance_accounts WHERE company_id=? AND status='active' AND id IN (${[...new Set(accountIds)].map(() => "?").join(",")})`).bind(actor.companyId, ...new Set(accountIds)).all<FinanceRow>()).results; },
        realizedFxAccounts: async (facts) => { const { actor, bankAccount, gain, input, loss } = facts; const write = await db(actor); return (await write.prepare(`SELECT id FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND currency=? AND ((id=? AND category='other_income') OR (id=? AND category='other_expense'))`).bind(actor.companyId, bankAccount.chart_id, bankAccount.base_currency, gain ? input.fxGainAccountId! : "", loss ? input.fxLossAccountId! : "").all<FinanceRow>()).results; },
        reversedPayment: async (facts) => { const { paymentId, actor } = facts; const write = await db(actor); return write.prepare("SELECT p.*,j.chart_id,j.status journal_status,j.journal_number,j.entry_date journal_entry_date FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id WHERE p.id=? AND p.company_id=?").bind(paymentId, actor.companyId).first<FinanceRow>(); },
        reversalAllocations: async (facts) => { const { paymentId, actor } = facts; const write = await db(actor); return (await write.prepare("SELECT a.*,i.total_minor,i.outstanding_minor,i.source_type FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE a.payment_id=? AND a.company_id=? ORDER BY a.id").bind(paymentId, actor.companyId).all<FinanceRow>()).results; },
        creditedSource: async (facts) => { const { paymentId, actor } = facts; const write = await db(actor); return !!await write.prepare("SELECT 1 FROM finance_billing_corrections c JOIN finance_allocations a ON a.invoice_id=c.invoice_id WHERE a.payment_id=? AND c.company_id=? AND c.status='posted'").bind(paymentId, actor.companyId).first(); },
        exportedPayments: async (facts) => { const { paymentIds, actor } = facts; const write = await db(actor); return (await write.prepare(`SELECT id,payment_number,payment_date,currency,outstanding_minor,withheld_minor,bank_account FROM finance_payments WHERE company_id=? AND status='released' AND id IN (${paymentIds.map(() => "?").join(",")}) ORDER BY payment_number`).bind(actor.companyId, ...paymentIds).all<FinanceRow>()).results; } });
}
function bankService(provided?: SqlDatabase,refundEvidence?:RefundBankEvidenceWitness,payrollEvidence?:PayrollOtherBankWitness) {
    let pending: Promise<SqlDatabase> | undefined;
    const db = (actor: WorkContext) => pending ??= resources(actor).then(runtime => {
        if (provided) {
            const owner = runtime.write as SqlDatabase & {
                assertActiveTransaction?: (db: SqlDatabase) => void;
            };
            if (!owner.assertActiveTransaction)
                throw new ApiError(503, 'TRANSACTION_OWNERSHIP_REQUIRED', 'Bank imports require an owned PostgreSQL transaction.');
            owner.assertActiveTransaction(provided);
        }
        return provided ?? runtime.write;
    });
    return createFinanceBank({ error: (...args) => new ApiError(...args), authorize: requirePermission, now: Date.now, digest: hash, ready: async (actor) => { await db(actor); }, replay: async (actor, key, operation, id, input) => {const result=await replayCommand(await db(actor), actor, key, operation, id, refundEvidence?refundBankEvidenceCommandInput(input as BankImportInput,refundEvidence):payrollEvidence?payrollOtherBankCommandInput(input as BankImportInput,payrollEvidence):input);return result?publicPayrollOtherBankResult(publicBankImportResult(result)):null;}, moneyScale: async (companyId, currency) => requireFinanceMoneyScale(await pending!, companyId, currency), functionalFx: async (actor, currency, baseCurrency, date) => confirmedFunctionalFxMicros(await db(actor), actor, currency, baseCurrency, date), account: async (actor, id) => (await db(actor)).prepare("SELECT a.currency,a.control_kind,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=? AND a.category='asset' AND a.control_kind IN ('none','bank') AND a.status='active' AND c.status='active' AND NOT EXISTS(SELECT 1 FROM finance_book_rules r WHERE r.company_id=a.company_id AND r.secondary_chart_id=a.chart_id AND r.status='approved')").bind(id, actor.companyId).first<{
            currency: string;
            control_kind: string;
            base_currency: string;
        }>(), commitImport: async ({ actor, input, key, id, opening, closing, amounts, closingFxRate, baseClosing, digest, now }) => {
            const write = await db(actor);
            const result={id,imported:input.rows.length,status:"imported",...(input.settlementPaymentId?{settlementPaymentId:input.settlementPaymentId}:{}),...(refundEvidence?{refundInstructionId:refundEvidence.instructionId,refundObligationId:refundEvidence.obligationId,bankTransactionId:`bankrow-${key}-0`}:{}),...(payrollEvidence?{payrollRunId:payrollEvidence.runId,kind:payrollEvidence.request.kind,liabilityAccountId:payrollEvidence.request.liabilityAccountId,bankTransactionId:`bankrow-${key}-0`}:{})};
            const receipt=refundEvidence?refundBankEvidenceStoredResult(result,refundBankEvidenceMarker({actor,input,key,id,opening,closing,amounts,closingFxRate,baseClosing,digest,now},refundEvidence)):payrollEvidence?payrollOtherBankStoredResult(result,{actor,input,key,id,opening,closing,amounts,closingFxRate,baseClosing,digest,now},payrollEvidence):result;
            return publicPayrollOtherBankResult(publicBankImportResult(await executeCommand(write, actor, key, "finance.bank.import", refundEvidence?refundBankEvidenceCommandInput(input,refundEvidence):payrollEvidence?payrollOtherBankCommandInput(input,payrollEvidence):input, id, 1, receipt, [
                assertion(write, key, "duplicate-statement", "NOT EXISTS(SELECT 1 FROM finance_bank_statements WHERE company_id=? AND bank_account=? AND (statement_number=? OR source_hash=?))", [actor.companyId, input.bankAccount, input.statementNumber, digest]),
                ...input.rows.map((r, i) => assertion(write, key, `external-${i}`, "NOT EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.company_id=? AND s.bank_account=? AND t.external_id=?)", [actor.companyId, input.bankAccount, r.externalId])),
                assertion(write, key, "bank-account", "EXISTS(SELECT 1 FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=? AND a.category='asset' AND a.status='active' AND a.control_kind IN ('none','bank') AND c.status='active' AND NOT EXISTS(SELECT 1 FROM finance_book_rules r WHERE r.company_id=a.company_id AND r.secondary_chart_id=a.chart_id AND r.status='approved'))", [input.bankAccount, actor.companyId]),
                write.prepare("UPDATE finance_accounts SET is_control=1,control_kind='bank',version=version+CASE WHEN control_kind='none' THEN 1 ELSE 0 END,updated_at=? WHERE id=? AND company_id=? AND control_kind='none'").bind(now, input.bankAccount, actor.companyId),
                write.prepare("INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,closing_fx_rate_micros,base_closing_minor,source_hash,imported_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(id, actor.companyId, input.bankAccount, input.statementNumber, input.statementDate, opening, closing, closingFxRate, baseClosing, digest, actor.userId, now),
                ...input.rows.map((r, i) => write.prepare("INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency) VALUES(?,?,?,?,?,?,?,?)").bind(`bankrow-${key}-${i}`, id, actor.companyId, r.transactionDate, r.externalId, r.description ?? "", amounts[i], r.currency.toUpperCase())),
            ])));
        }, commitReconciliation: async ({ actor, transactionId, input, key, now }) => {
            const write = await db(actor);
            const attachmentBinding = input.attachmentSetIds.length ? await prepareAttachmentBinding(write, actor, { attachmentSetIds: input.attachmentSetIds, bindingContract: { targetType: "finance_bank_transaction", bindingType: "finance_bank_reconciliation", contexts: [{ purposeKey: "reconciliation_workpaper", componentKind: "action", componentRef: "reconcile" }] }, targetId: transactionId, expectedTargetVersion: input.expectedVersion, commandId: key, bindingId: `${transactionId}:${input.expectedVersion + 1}` }) : null;
            return executeCommand(write, actor, key, "finance.bank.reconcile", input, transactionId, input.expectedVersion + 1, { id: transactionId, status: "matched", version: input.expectedVersion + 1 }, [
                ...(attachmentBinding?.guards ?? []),
                assertion(write, key, "payment-match", "EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id JOIN finance_payments p ON p.id=? JOIN finance_journals j ON j.id=p.journal_id WHERE t.id=? AND t.company_id=? AND p.company_id=t.company_id AND t.version=? AND t.match_status IN ('unmatched','proposed') AND p.currency=t.currency AND p.bank_account=s.bank_account AND p.status IN ('released','settled') AND j.status='posted' AND t.amount_minor=CASE WHEN p.kind='receipt' THEN p.amount_minor ELSE -(p.amount_minor-p.withheld_minor) END)", [input.matchedId, transactionId, actor.companyId, input.expectedVersion]),
                assertion(write, key, "already-matched", "NOT EXISTS(SELECT 1 FROM finance_bank_transactions WHERE company_id=? AND matched_type='payment' AND matched_id=? AND match_status='matched')", [actor.companyId, input.matchedId]),
                assertion(write, key, "group-already-matched", "NOT EXISTS(SELECT 1 FROM finance_bank_match_targets t JOIN finance_journal_lines l ON l.id=t.journal_line_id JOIN finance_payments p ON p.journal_id=l.journal_id WHERE p.id=? AND p.company_id=?)", [input.matchedId, actor.companyId]),
                write.prepare("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?").bind(input.matchedId, transactionId),
                ...(attachmentBinding?.steps ?? []),
                financeOwnerDecisionEffects(write).appendReviewedDecision({id:crypto.randomUUID(),commandId:key,recordId:transactionId,action:"finance.bank.reconcile",actorId:actor.userId,assignmentId:actor.assignmentId,reviewedVersion:input.expectedVersion,inputDigest:await hash(input),createdAt:now}),
            ]);
        } });
}
function fxService(provided?: SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined = provided ? Promise.resolve(provided) : undefined;
    const db = (actor: WorkContext, readOnly = false) => pending ??= resources(actor, readOnly).then(r => r.write);
    const journals = new Map<string, Awaited<ReturnType<typeof prepareJournal>>>();
    return createFxRevaluation({ error: (...args) => new ApiError(...args), authorize: requirePermission, now: Date.now, id: () => crypto.randomUUID(), digest: hash, ready: async (actor, readOnly) => { await db(actor, readOnly); }, prior: async (actor, key, operation, input) => priorCommand(await db(actor), actor, key, operation, input), replay: async (actor, key, operation, id, input) => replayCommand(await db(actor), actor, key, operation, id, input), prepareJournal: async (actor, input, key, status) => { const write = await db(actor), plan = await journalPreparation(write)(actor, input, key, status); journals.set(plan.result.id, journalStatements(write, actor, input, key, status, plan)); return plan; }, exposureChart: async (facts) => { const { chartId, actor } = facts; const write = await db(actor); return write.prepare("SELECT id,base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'").bind(chartId, actor.companyId).first<FinanceRow>(); },
        exposureRows: async (facts) => {
            const { period, actor, chartId, chart } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT i.id,i.kind,i.currency,i.total_minor,i.base_total_minor,i.fx_rate_micros,i.version,
    CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END control_account_id,
    i.total_minor-COALESCE((SELECT SUM(a.amount_minor) FROM finance_allocations a JOIN finance_payments p ON p.id=a.payment_id WHERE a.invoice_id=i.id AND p.status<>'voided' AND p.payment_date<=?),0) transaction_outstanding,
    (CASE WHEN i.base_total_minor=0 AND i.total_minor>0 AND i.fx_rate_micros=1000000 THEN i.total_minor ELSE i.base_total_minor END)
      -COALESCE((SELECT SUM(CASE WHEN a.base_invoice_minor=0 THEN a.amount_minor ELSE a.base_invoice_minor END) FROM finance_allocations a JOIN finance_payments p ON p.id=a.payment_id WHERE a.invoice_id=i.id AND p.status<>'voided' AND p.payment_date<=?),0)
      +COALESCE((SELECT SUM(l.adjustment_minor) FROM finance_fx_revaluation_lines l JOIN finance_fx_revaluation_runs r ON r.id=l.run_id JOIN finance_periods rp ON rp.id=r.period_id WHERE l.invoice_id=i.id AND r.status='posted' AND rp.end_date<?),0) prior_base
    FROM finance_invoices i JOIN finance_accounts a ON a.id=CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END
    WHERE i.company_id=? AND a.chart_id=? AND i.currency<>? AND i.invoice_date<=? AND i.status IN ('posted','partially_paid','paid')
    ORDER BY i.currency,i.invoice_number,i.id`).bind(period.end_date, period.end_date, period.end_date, actor.companyId, chartId, chart.base_currency, period.end_date).all<FinanceRow>()).results;
        },
        workspacePeriod: async (facts) => { const { periodId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId, actor.companyId).first<FinanceRow>(); },
        workspaceCurrencies: async (facts) => { const { actor, period } = facts; const write = await db(actor); return (await write.prepare(`SELECT DISTINCT a.chart_id,c.name chart_name,c.base_currency,i.currency FROM finance_invoices i JOIN finance_accounts a ON a.id=CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END JOIN finance_charts c ON c.id=a.chart_id WHERE i.company_id=? AND i.currency<>c.base_currency AND i.invoice_date<=? AND i.status IN ('posted','partially_paid') AND i.outstanding_minor>0 ORDER BY c.name,i.currency`).bind(actor.companyId, period.end_date).all<FinanceRow>()).results; },
        workspaceAccounts: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT a.id,a.chart_id,a.code,a.name,a.category,a.currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.status='active' AND a.currency=c.base_currency AND a.category IN ('other_income','other_expense') ORDER BY a.code").bind(actor.companyId).all<FinanceRow>()).results; },
        workspaceRuns: async (facts) => { const { actor, periodId } = facts; const write = await db(actor); return (await write.prepare("SELECT r.*,c.name chart_name FROM finance_fx_revaluation_runs r JOIN finance_charts c ON c.id=r.chart_id WHERE r.company_id=? AND r.period_id=? ORDER BY r.created_at DESC").bind(actor.companyId, periodId).all<FinanceRow>()).results; },
        openPeriod: async (facts) => { const { input, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=? AND state='open'").bind(input.periodId, actor.companyId).first<FinanceRow>(); },
        validAdjustmentAccounts: async (facts) => { const { actor, input, calculated } = facts; const write = await db(actor); return (await write.prepare("SELECT id,category FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND currency=? AND id IN (?,?)").bind(actor.companyId, input.chartId, calculated.chart.base_currency, input.gainAccountId, input.lossAccountId).all<FinanceRow>()).results; },
        commitPreparation: async (facts) => {
            const { actor, key, input, output, runId, calculated, rates, now } = facts;
            const write = await db(actor);
            return saveCommand(write, actor, key, "finance.fx-revaluation.prepare", input, output, [
                assertion(write, key, "period", "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND end_date=?)", [input.periodId, actor.companyId, input.entryDate]),
                assertion(write, key, "posted-run", "NOT EXISTS(SELECT 1 FROM finance_fx_revaluation_runs WHERE period_id=? AND chart_id=? AND status='posted')", [input.periodId, input.chartId]),
                write.prepare("INSERT INTO finance_fx_revaluation_runs(id,company_id,chart_id,period_id,entry_date,base_currency,rates_json,source_hash,gain_account_id,loss_account_id,preparation_proof,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(runId, actor.companyId, input.chartId, input.periodId, input.entryDate, calculated.chart.base_currency, JSON.stringify(rates), calculated.sourceHash, input.gainAccountId, input.lossAccountId, input.proof, actor.userId, now, now),
                ...calculated.exposures.map((row, index) => write.prepare("INSERT INTO finance_fx_revaluation_lines(id,run_id,invoice_id,kind,currency,control_account_id,transaction_outstanding_minor,prior_base_minor,revalued_base_minor,adjustment_minor,rate_micros) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(`${runId}-${index}`, runId, row.invoiceId, row.kind, row.currency, row.controlAccountId, row.transactionOutstanding, row.priorBase, row.revaluedBase, row.adjustment, row.rateMicros)),
            ]);
        },
        decidedRun: async (facts) => { const { runId, actor } = facts; const write = await db(actor); return write.prepare("SELECT r.*,p.state period_state,p.end_date FROM finance_fx_revaluation_runs r JOIN finance_periods p ON p.id=r.period_id WHERE r.id=? AND r.company_id=?").bind(runId, actor.companyId).first<FinanceRow>(); },
        commitReject: async (facts) => { const { actor, key, input, runId, now } = facts; const write = await db(actor); return executeCommand(write, actor, key, "finance.fx-revaluation.reject", input, runId, 2, { id: runId, status: "rejected", version: 2 }, [assertion(write, key, "run", "EXISTS(SELECT 1 FROM finance_fx_revaluation_runs WHERE id=? AND company_id=? AND status='prepared' AND version=1 AND created_by!=?)", [runId, actor.companyId, actor.userId]), write.prepare("UPDATE finance_fx_revaluation_runs SET status='rejected',decision_proof=?,approved_by=?,version=2,updated_at=? WHERE id=?").bind(input.proof, actor.userId, now, runId), financeOwnerDecisionEffects(write).appendRetainedLayoutDecision({id:crypto.randomUUID(),commandId:key,recordId:runId,action:"finance.fx-revaluation.reject",actorId:actor.userId,assignmentId:actor.assignmentId,reviewedVersion:1,inputDigest:await hash(input),createdAt:now})]); },
        runPeriod: async (facts) => { const { run, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_periods WHERE id=?").bind(run.period_id).first<FinanceRow>(); },
        commitApproval: async (facts) => {
            const { actor, key, input, runId, journal: plan, gain, loss, run, calculated, now } = facts;
            const write = await db(actor);
            const journal = plan ? journals.get(plan.result.id)! : null;
            return executeCommand(write, actor, key, "finance.fx-revaluation.approve", input, runId, 2, { id: runId, status: "posted", version: 2, journalId: journal?.result.id ?? null, gain: minorToDecimal(gain), loss: minorToDecimal(loss) }, [
                assertion(write, key, "run", "EXISTS(SELECT 1 FROM finance_fx_revaluation_runs r JOIN finance_periods p ON p.id=r.period_id WHERE r.id=? AND r.company_id=? AND r.status='prepared' AND r.version=1 AND r.created_by!=? AND p.state='open')", [runId, actor.companyId, actor.userId]),
                assertion(write, key, "posted-run", "NOT EXISTS(SELECT 1 FROM finance_fx_revaluation_runs WHERE period_id=? AND chart_id=? AND status='posted')", [run.period_id, run.chart_id]),
                ...calculated.exposures.map((row, index) => assertion(write, key, `invoice-${index}`, "EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND version=? AND outstanding_minor=?)", [row.invoiceId, actor.companyId, row.version, row.transactionOutstanding])),
                ...(journal?.steps ?? []),
                ...calculated.exposures.map(row => write.prepare("UPDATE finance_invoices SET base_outstanding_minor=base_outstanding_minor+?,version=version+1,updated_at=? WHERE id=?").bind(row.adjustment, now, row.invoiceId)),
                write.prepare("UPDATE finance_fx_revaluation_runs SET status='posted',journal_id=?,decision_proof=?,approved_by=?,version=2,updated_at=? WHERE id=?").bind(journal?.result.id ?? null, input.proof, actor.userId, now, runId),
                financeOwnerDecisionEffects(write).appendRetainedLayoutDecision({id:crypto.randomUUID(),commandId:key,recordId:runId,action:"finance.fx-revaluation.approve",actorId:actor.userId,assignmentId:actor.assignmentId,reviewedVersion:1,inputDigest:await hash(input),createdAt:now}),
            ]);
        } });
}
function closeService(provided?: SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined = provided ? Promise.resolve(provided) : undefined;
    const db = (actor: WorkContext) => pending ??= resources(actor).then(r => r.write);
    return createFinanceClose({ error: (...args) => new ApiError(...args), authorize: requirePermission, now: Date.now, digest: hash, ready: async (actor) => { await db(actor); }, replay: async (actor, key, operation, id, input) => replayCommand(await db(actor), actor, key, operation, id, input), vatEvidence: async (actor, date) => calculateVatCloseEvidence(await db(actor), actor, date), expansionPending: async (companyId, start, end) => financeExpansionClosePending(await pending!, companyId, start, end), commitClose: async ({ actor, periodId, input, id, period, reconciliation, version, now }) => {
            const write = await db(actor);
            const attachmentBinding = input.attachmentSetIds.length ? await prepareAttachmentBinding(write, actor, { attachmentSetIds: input.attachmentSetIds, bindingContract: { targetType: "finance_period", bindingType: "finance_period_close", contexts: [{ purposeKey: "close_workpaper", componentKind: "action", componentRef: "close-approval" }] }, targetId: periodId, expectedTargetVersion: version, commandId: id, bindingId: `${periodId}:${version + 1}` }) : null;
            return executeCommand(write, actor, id, "finance.period.close", input, periodId, version + 1, { id: periodId, state: "closed", version: version + 1 }, [
                ...(attachmentBinding?.guards ?? []),
                assertion(write, id, "period", "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='closing' AND close_version=? AND close_started_by!=?)", [periodId, actor.companyId, version, actor.userId]),
                ...refundCloseGuards(write, actor, period, id),
                assertion(write, id, "close-journals", "NOT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=? AND (period_id=? OR entry_date BETWEEN ? AND ?) AND status IN ('draft','in_review'))", [actor.companyId, periodId, period.start_date, period.end_date]),
                assertion(write, id, "close-invoices", "NOT EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=? AND status IN ('draft','in_review') AND invoice_date BETWEEN ? AND ?)", [actor.companyId, period.start_date, period.end_date]),
                assertion(write, id, "close-bank", "NOT EXISTS(SELECT 1 FROM finance_bank_transactions WHERE company_id=? AND match_status IN ('unmatched','proposed') AND transaction_date BETWEEN ? AND ?)", [actor.companyId, period.start_date, period.end_date]),
                financeCloseManufacturingSource(write).guard(id,"close-manufacturing",actor.companyId),
                assertion(write, id, "close-calculations", "NOT EXISTS(SELECT 1 FROM finance_calculation_runs WHERE company_id=? AND period_id=? AND status IN ('prepared','reviewed','stale'))", [actor.companyId, periodId]),
                assertion(write, id, "close-vat-review", `(${pendingVatReviewSql})=0`, [period.end_date, actor.companyId]),
                assertion(write, id, "close-finance-source-work", `(${financeExpansionCloseCountSql})=0`, [actor.companyId, period.start_date, period.end_date]),
                ...reconciliationAssertions(write, actor, period, id, reconciliation.rows),
                ...vatCloseGuards(write, actor, id, String(period.end_date), reconciliation.vat),
                write.prepare("UPDATE finance_periods SET state='closed',closed_by=?,closed_at=?,close_version=close_version+1 WHERE id=?").bind(actor.userId, now, periodId),
                ...(attachmentBinding?.steps ?? []),
                financeOwnerDecisionEffects(write).appendRetainedLayoutDecision({id:crypto.randomUUID(),commandId:id,recordId:periodId,action:"finance.period.close",actorId:actor.userId,assignmentId:actor.assignmentId,reviewedVersion:version,inputDigest:await hash(input),createdAt:now}),
            ]);
        }, controlAccounts: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT a.id,a.code,a.name,a.currency,a.control_kind,c.name chart_name,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.status='active' AND c.status='active' AND a.control_kind IN ('receivable','payable','bank') ORDER BY c.name,a.code").bind(actor.companyId).all<FinanceRow>()).results; },
        primaryBookAccount: async (facts) => {
            const { actor, accountId, period } = facts;
            const write = await db(actor);
            return write.prepare(`SELECT m.primary_account_id FROM finance_book_account_mappings m JOIN finance_book_rules r ON r.id=m.rule_id
      WHERE r.company_id=? AND r.status='approved' AND m.secondary_account_id=? AND r.effective_date<=? ORDER BY r.effective_date DESC LIMIT 1`).bind(actor.companyId, accountId, period.end_date).first<{
                primary_account_id: string;
            }>();
        },
        ledgerSigned: async (facts) => { const { actor, accountId, period } = facts; const write = await db(actor); return Number(await write.prepare(`SELECT COALESCE(SUM(l.base_debit_minor-l.base_credit_minor),0) value FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND l.account_id=? AND j.status IN ('posted','reversed') AND j.entry_date<=?`).bind(actor.companyId, accountId, period.end_date).first("value") ?? 0); },
        bankClosing: async (facts) => { const { actor, sourceAccountId, period } = facts; const write = await db(actor); return write.prepare("SELECT id,statement_number,statement_date,closing_minor,closing_fx_rate_micros,base_closing_minor,status FROM finance_bank_statements WHERE company_id=? AND bank_account=? AND statement_date<=? ORDER BY statement_date DESC,created_at DESC LIMIT 1").bind(actor.companyId, sourceAccountId, period.end_date).first<FinanceRow>(); },
        subledgerClosing: async (facts) => {
            const { actor, period, account, kind, sourceAccountId } = facts;
            const write = await db(actor);
            return write.prepare(`${invoiceHistoryCte}
      SELECT COALESCE(SUM(i.historical_base_outstanding_minor),0) value,COUNT(*) invoice_count,
        COUNT(*) FILTER(WHERE i.currency<>? AND i.historical_outstanding_minor>0 AND NOT EXISTS(
          SELECT 1 FROM finance_fx_revaluation_lines l JOIN finance_fx_revaluation_runs r ON r.id=l.run_id
          WHERE l.invoice_id=i.id AND r.period_id=? AND r.status='posted')) foreign_open_count,
        COUNT(*) FILTER(WHERE i.legacy_date_evidence) legacy_date_count
      FROM historical_invoice_balances i WHERE i.kind=? AND
        CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END=?`)
                .bind(actor.companyId, period.end_date, account.base_currency, period.id, kind, sourceAccountId).first<FinanceRow>();
        },
        beginPeriod: async (facts) => { const { periodId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId, actor.companyId).first<FinanceRow>(); },
        draftJournals: async (facts) => {
            const { actor, periodId, period } = facts;
            const write = await db(actor);
            return write.prepare("SELECT COUNT(*) AS count FROM finance_journals WHERE company_id=? AND (period_id=? OR entry_date BETWEEN ? AND ?) AND status IN ('draft','in_review')").bind(actor.companyId, periodId, period.start_date, period.end_date).first<{
                count: number;
            }>();
        },
        draftInvoices: async (facts) => {
            const { actor, period } = facts;
            const write = await db(actor);
            return write.prepare("SELECT COUNT(*) AS count FROM finance_invoices WHERE company_id=? AND status IN ('draft','in_review') AND invoice_date BETWEEN ? AND ?").bind(actor.companyId, period.start_date, period.end_date).first<{
                count: number;
            }>();
        },
        unmatchedBank: async (facts) => {
            const { actor, period } = facts;
            const write = await db(actor);
            return write.prepare("SELECT COUNT(*) AS count FROM finance_bank_transactions WHERE company_id=? AND match_status IN ('unmatched','proposed') AND transaction_date BETWEEN ? AND ?").bind(actor.companyId, period.start_date, period.end_date).first<{
                count: number;
            }>();
        },
        openManufacturing: async (facts) => {
            const { actor } = facts;
            const write = await db(actor);
            return financeCloseManufacturingSource(write).count(actor.companyId);
        },
        pendingCalculations: async (facts) => {
            const { actor, periodId } = facts;
            const write = await db(actor);
            return write.prepare("SELECT COUNT(*) count FROM finance_calculation_runs WHERE company_id=? AND period_id=? AND status IN ('prepared','reviewed','stale')").bind(actor.companyId, periodId).first<{
                count: number;
            }>();
        },
        pendingTax: async (facts) => { const { period, actor } = facts; const write = await db(actor); return Number(await write.prepare(pendingVatReviewSql).bind(period.end_date, actor.companyId).first("value") ?? 0); },
        pendingSupplierBills: async (facts) => { const { actor, period } = facts; const write = await db(actor); return financeCloseProcurementSource(write).count(actor.companyId,period.end_date); },
        pendingSupplierBank: async (facts) => {
            const { actor, period } = facts;
            const write = await db(actor);
            return write.prepare("SELECT COUNT(*) AS count FROM finance_supplier_instructions i JOIN finance_supplier_batches b ON b.id=i.batch_id WHERE b.company_id=? AND i.status IN ('exported','unknown') AND b.planned_date<=?").bind(actor.companyId, period.end_date).first<{
                count: number;
            }>();
        },
        pendingRefunds: async (facts) => {
            const { actor, period } = facts;
            const write = await db(actor);
            return write.prepare(`SELECT (${financeCloseCommerceSource(write).pendingRefundCount}) returns,(SELECT COUNT(*) FROM finance_refund_instructions WHERE company_id=? AND status IN ('exported','unknown') AND planned_date<=?) instructions`).bind(actor.companyId, period.end_date, actor.companyId, period.end_date).first<{
                returns: number;
                instructions: number;
            }>();
        },
        commitBegin: async (facts) => {
            const { actor, id, periodId, nextVersion, reconciliation, period, readiness, checkSource, checkedAt } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, id, "finance.period.begin-close", { periodId }, periodId, nextVersion, { id: periodId, state: "closing", version: nextVersion, reconciliationHash: reconciliation.reconciliationHash, reconciliations: reconciliation.rows }, [
                assertion(write, id, "period", "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND close_version=?)", [periodId, actor.companyId, period.close_version]),
                assertion(write, id, "journals", "NOT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=? AND (period_id=? OR entry_date BETWEEN ? AND ?) AND status IN ('draft','in_review'))", [actor.companyId, periodId, period.start_date, period.end_date]),
                assertion(write, id, "invoices", "NOT EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=? AND status IN ('draft','in_review') AND invoice_date BETWEEN ? AND ?)", [actor.companyId, period.start_date, period.end_date]),
                assertion(write, id, "bank", "NOT EXISTS(SELECT 1 FROM finance_bank_transactions WHERE company_id=? AND match_status IN ('unmatched','proposed') AND transaction_date BETWEEN ? AND ?)", [actor.companyId, period.start_date, period.end_date]),
                financeCloseManufacturingSource(write).guard(id,"manufacturing",actor.companyId),
                assertion(write, id, "calculations", "NOT EXISTS(SELECT 1 FROM finance_calculation_runs WHERE company_id=? AND period_id=? AND status IN ('prepared','reviewed','stale'))", [actor.companyId, periodId]),
                assertion(write, id, "vat-review", `(${pendingVatReviewSql})=0`, [period.end_date, actor.companyId]),
                assertion(write, id, "finance-source-work", `(${financeExpansionCloseCountSql})=0`, [actor.companyId, period.start_date, period.end_date]),
                ...refundCloseGuards(write, actor, period, id),
                ...reconciliationAssertions(write, actor, period, id, reconciliation.rows),
                ...vatCloseGuards(write, actor, id, String(period.end_date), reconciliation.vat),
                ...Object.entries(readiness).map(([check, count]) => write.prepare("INSERT INTO finance_close_checks(id,company_id,period_id,check_key,source_version,status,count_value,detail_json,checked_at,checked_by) VALUES(?,?,?,?,?,'pass',?,?,?,?) ON CONFLICT(period_id,check_key,source_version) DO NOTHING").bind(crypto.randomUUID(), actor.companyId, periodId, check, checkSource, count, JSON.stringify({ count }), checkedAt, actor.userId)),
                ...(reconciliation.vat ? [write.prepare("INSERT INTO finance_close_checks(id,company_id,period_id,check_key,source_version,status,count_value,detail_json,checked_at,checked_by) VALUES(?,?,?,?,?,'pass',0,?,?,?) ON CONFLICT(period_id,check_key,source_version) DO NOTHING").bind(crypto.randomUUID(), actor.companyId, periodId, "statutory-vat", checkSource, JSON.stringify(reconciliation.vat), checkedAt, actor.userId)] : []),
                ...reconciliation.rows.map(row => write.prepare("INSERT INTO finance_close_reconciliations(id,company_id,period_id,close_version,kind,account_id,currency,ledger_minor,source_minor,difference_minor,source_json,checked_at,checked_by) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(), actor.companyId, periodId, nextVersion, row.kind, row.accountId, row.currency, row.ledgerMinor, row.sourceMinor, row.differenceMinor, JSON.stringify(row.source), checkedAt, actor.userId)),
                write.prepare("UPDATE finance_periods SET state='closing',close_started_by=?,close_version=close_version+1,reconciliation_hash=? WHERE id=?").bind(actor.userId, reconciliation.reconciliationHash, periodId),
            ]);
        },
        cancelledPeriod: async (facts) => { const { periodId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId, actor.companyId).first<FinanceRow>(); },
        commitCancel: async (facts) => {
            const { actor, id, input, periodId } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, id, "finance.period.cancel-close", input, periodId, input.expectedVersion + 1, { id: periodId, state: "open", version: input.expectedVersion + 1 }, [
                assertion(write, id, "period", "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='closing' AND close_version=?)", [periodId, actor.companyId, input.expectedVersion]),
                write.prepare("UPDATE finance_periods SET state='open',close_started_by=NULL,reconciliation_hash=NULL,close_version=close_version+1 WHERE id=? AND company_id=?").bind(periodId, actor.companyId),
                financeOwnerDecisionEffects(write).appendRetainedLayoutDecision({id:crypto.randomUUID(),commandId:id,recordId:periodId,action:"finance.period.cancel-close",actorId:actor.userId,assignmentId:actor.assignmentId,reviewedVersion:input.expectedVersion,inputDigest:await hash(input),createdAt:Date.now()}),
            ]);
        },
        closedPeriod: async (facts) => { const { periodId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId, actor.companyId).first<FinanceRow>(); },
        reopenedPeriod: async (facts) => { const { periodId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=? AND state='closed'").bind(periodId, actor.companyId).first<FinanceRow>(); },
        commitReopen: async (facts) => {
            const { actor, id, input, periodId, version } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, id, "finance.period.reopen", input, periodId, version + 1, { id: periodId, state: "open", version: version + 1 }, [
                assertion(write, id, "period", "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND state='closed' AND close_version=?)", [periodId, version]),
                write.prepare("UPDATE finance_periods SET state='open',closed_by=NULL,closed_at=NULL,reconciliation_hash=NULL,close_version=close_version+1 WHERE id=?").bind(periodId),
                financeOwnerDecisionEffects(write).appendRetainedLayoutDecision({id:crypto.randomUUID(),commandId:id,recordId:periodId,action:"finance.period.reopen",actorId:actor.userId,assignmentId:actor.assignmentId,reviewedVersion:version,inputDigest:await hash(input),createdAt:Date.now()}),
            ]);
        } });
}
function legacyRevenueService() {
    let pending: Promise<SqlDatabase> | undefined;
    const db = (actor: WorkContext) => pending ??= resources(actor).then(r => r.write);
    return createLegacyRevenue({ error: (...args) => new ApiError(...args), authorize: requirePermission, ready: async (actor) => { await db(actor); }, prior: async (actor, key, operation, input) => priorCommand(await db(actor), actor, key, operation, input), obligation: async (actor, obligationId) => (await db(actor)).prepare('SELECT o.*,c.company_id,c.currency FROM finance_revenue_obligations o JOIN finance_revenue_contracts c ON c.id=o.contract_id WHERE o.id=? AND c.company_id=?').bind(obligationId, actor.companyId).first<FinanceRow>(), account: async (actor, id) => (await db(actor)).prepare('SELECT chart_id FROM finance_accounts WHERE id=? AND company_id=?').bind(id, actor.companyId).first<{
            chart_id: string;
        }>(), createJournal, postJournal, commit: async ({ actor, obligationId, input, id, amount, output }) => { const write = await db(actor), pendingStatements: SqlStatement[] = []; pendingStatements.push(write.prepare("UPDATE finance_revenue_obligations SET recognized_minor=recognized_minor+?,status=CASE WHEN recognized_minor+?=allocated_minor THEN 'complete' ELSE 'active' END WHERE id=?").bind(amount, amount, obligationId)); return saveCommand(write, actor, id, 'finance.revenue.recognize', input, output, pendingStatements); } });
}

export async function createManufacturingPostingChart(actor:WorkContext,raw:unknown,key:string){const request=manufacturingPostingChartSchema.parse(raw);return withManufacturingPostingPrerequisiteCommand(actor,'chart',request,key,tx=>configurationService(tx,request).createChart(actor,request.input,key));}
export async function createManufacturingPostingAccount(actor:WorkContext,raw:unknown,key:string){const request=manufacturingPostingAccountSchema.parse(raw);return withManufacturingPostingPrerequisiteCommand(actor,'account',request,key,tx=>configurationService(tx,request).createAccount(actor,request.input,key));}
export async function createManufacturingPostingPeriod(actor:WorkContext,raw:unknown,key:string){const request=manufacturingPostingPeriodSchema.parse(raw);return withManufacturingPostingPrerequisiteCommand(actor,'period',request,key,tx=>configurationService(tx,request).createPeriod(actor,request.input,key));}

import {posStartupChartSchema,posStartupAccountSchema,posStartupPeriodSchema} from '../domain/pos-startup-prerequisites';
import {withPosStartupPrerequisiteCommand} from './pos-startup-prerequisites';
export async function createPosStartupChart(actor:WorkContext,raw:unknown,key:string){const request=posStartupChartSchema.parse(raw);return withPosStartupPrerequisiteCommand(actor,'chart',request,key,tx=>configurationService(tx,request).createChart(actor,request.input,key));}
export async function createPosStartupAccount(actor:WorkContext,raw:unknown,key:string){const request=posStartupAccountSchema.parse(raw);return withPosStartupPrerequisiteCommand(actor,'account',request,key,tx=>configurationService(tx,request).createAccount(actor,request.input,key));}
export async function createPosStartupPeriod(actor:WorkContext,raw:unknown,key:string){const request=posStartupPeriodSchema.parse(raw);return withPosStartupPrerequisiteCommand(actor,'period',request,key,tx=>configurationService(tx,request).createPeriod(actor,request.input,key));}

import {assetsPostingChartSchema,assetsPostingAccountSchema,assetsPostingPeriodSchema} from '../domain/assets-posting-prerequisites';
import {withAssetsPostingPrerequisiteCommand} from './assets-posting-prerequisites';
export async function createAssetsPostingChart(actor:WorkContext,raw:unknown,key:string){const request=assetsPostingChartSchema.parse(raw);return withAssetsPostingPrerequisiteCommand(actor,'chart',request,key,tx=>configurationService(tx,request).createChart(actor,request.input,key));}
export async function createAssetsPostingAccount(actor:WorkContext,raw:unknown,key:string){const request=assetsPostingAccountSchema.parse(raw);return withAssetsPostingPrerequisiteCommand(actor,'account',request,key,tx=>configurationService(tx,request).createAccount(actor,request.input,key));}
export async function createAssetsPostingPeriod(actor:WorkContext,raw:unknown,key:string){const request=assetsPostingPeriodSchema.parse(raw);return withAssetsPostingPrerequisiteCommand(actor,'period',request,key,tx=>configurationService(tx,request).createPeriod(actor,request.input,key));}

import {servicePartsChartSchema,servicePartsAccountSchema,servicePartsPeriodSchema} from '../domain/service-parts-prerequisites';
import {withServicePartsPrerequisiteCommand} from './service-parts-prerequisites';
export async function createServicePartsChart(actor:WorkContext,raw:unknown,key:string){const request=servicePartsChartSchema.parse(raw);return withServicePartsPrerequisiteCommand(actor,'chart',request,key,tx=>configurationService(tx,request).createChart(actor,request.input,key));}
export async function createServicePartsAccount(actor:WorkContext,raw:unknown,key:string){const request=servicePartsAccountSchema.parse(raw);return withServicePartsPrerequisiteCommand(actor,'account',request,key,tx=>configurationService(tx,request).createAccount(actor,request.input,key));}
export async function createServicePartsPeriod(actor:WorkContext,raw:unknown,key:string){const request=servicePartsPeriodSchema.parse(raw);return withServicePartsPrerequisiteCommand(actor,'period',request,key,tx=>configurationService(tx,request).createPeriod(actor,request.input,key));}

/** Closed Payroll references retain the original Finance application, native IDs and receipts. */
export async function createPayrollPostingChart(actor:WorkContext,raw:unknown,key:string):Promise<PayrollPostingChartResult>{const request=payrollPostingChartSchema.parse(raw);return withPayrollPostingPrerequisiteCommand(actor,'chart',request,key,async tx=>{const result=await configurationService(tx,request).createChart(actor,request.input,key);if(result.version!==1)throw new ApiError(409,'PAYROLL_POSTING_SOURCE_CHANGED','The original chart receipt is incomplete.');return {...result,version:result.version};});}
export async function createPayrollPostingAccount(actor:WorkContext,raw:unknown,key:string):Promise<PayrollPostingAccountResult>{const request=payrollPostingAccountSchema.parse(raw);return withPayrollPostingPrerequisiteCommand(actor,'account',request,key,async tx=>{const result=await configurationService(tx,request).createAccount(actor,request.input,key);if(result.version!==1)throw new ApiError(409,'PAYROLL_POSTING_SOURCE_CHANGED','The original account receipt is incomplete.');return {...result,version:result.version};});}
export async function createPayrollPostingPeriod(actor:WorkContext,raw:unknown,key:string):Promise<PayrollPostingPeriodResult>{const request=payrollPostingPeriodSchema.parse(raw);return withPayrollPostingPrerequisiteCommand(actor,'period',request,key,async tx=>{const result=await configurationService(tx,request).createPeriod(actor,request.input,key);if(result.version!==0||result.state!=='open')throw new ApiError(409,'PAYROLL_POSTING_SOURCE_CHANGED','The original period receipt is incomplete.');return {...result,state:result.state,version:result.version};});}

/** Closed Expense references retain the original Finance application, native IDs and receipts. */
export async function createExpensePostingChart(actor:WorkContext,raw:unknown,key:string):Promise<ExpensePostingChartResult>{const request=expensePostingChartSchema.parse(raw);return withExpensePostingPrerequisiteCommand(actor,'chart',request,key,async tx=>{const result=await configurationService(tx,request).createChart(actor,request.input,key);if(result.version!==1)throw new ApiError(409,'EXPENSE_POSTING_SOURCE_CHANGED','The original chart receipt is incomplete.');return {...result,version:result.version};});}
export async function createExpensePostingAccount(actor:WorkContext,raw:unknown,key:string):Promise<ExpensePostingAccountResult>{const request=expensePostingAccountSchema.parse(raw);return withExpensePostingPrerequisiteCommand(actor,'account',request,key,async tx=>{const result=await configurationService(tx,request).createAccount(actor,request.input,key);if(result.version!==1)throw new ApiError(409,'EXPENSE_POSTING_SOURCE_CHANGED','The original account receipt is incomplete.');return {...result,version:result.version};});}
export async function createExpensePostingPeriod(actor:WorkContext,raw:unknown,key:string):Promise<ExpensePostingPeriodResult>{const request=expensePostingPeriodSchema.parse(raw);return withExpensePostingPrerequisiteCommand(actor,'period',request,key,async tx=>{const result=await configurationService(tx,request).createPeriod(actor,request.input,key);if(result.version!==0||result.state!=='open')throw new ApiError(409,'EXPENSE_POSTING_SOURCE_CHANGED','The original period receipt is incomplete.');return {...result,state:result.state,version:result.version};});}

/** Service customer creation retains the original Finance schema and owner. */
export async function createServiceCustomer(actor:WorkContext,raw:unknown,key:string){const request=serviceCustomerSchema.parse(raw);return(await withServiceCustomerCommand(actor,request,key,async tx=>{const result=await configurationService(tx,request).createParty(actor,request.input,key);if(result.kind!=='customer')throw new ApiError(409,'SERVICE_CUSTOMER_SOURCE_CHANGED','The original customer receipt is incomplete.');return{...result,kind:result.kind};})).result;}
