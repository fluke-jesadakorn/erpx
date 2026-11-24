import { REFUND_INSTRUCTION_POINT_SQL } from './finance-read-tail-r38-sql';
import { idCompanyStatusVersionGuard } from './finance-read-tail-r20-sql';
import { ACCOUNT_SELECT_PREFIX } from './finance-read-tail-r30-sql';
import {materializedClockCteSql} from './finance-read-tail-r19-sql';
import {withRefundBankEvidenceConfirmation,withRefundBankEvidenceWorkspace,type RefundBankEvidenceReadScope} from '@erpx/server-finance/adapters/refund-bank-evidence';
import { BANK_TRANSACTION_BY_ID_SQL } from './finance-read-tail-r17-sql';
import { OPEN_PERIOD_LIST_ASC_SQL } from './finance-read-tail-r34-sql';
import { nativeFinanceRefunds } from '@erpx/server-collaboration/adapters/native-finance-refunds';
import {withFinancePurchasedRead} from "@erpx/server-finance/adapters/purchase-boundary";
import {financeRefundHistoryPredicate} from "@erpx/server-finance/adapters/document-purchases";
import {financePackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";
import {financeSourceAccess} from "@erpx/server-finance/adapters/source-purchases";
import {withRefundPurchasedRead,withRefundPurchasedCommand,withServiceRefundCashPreparation,refundSourcePurchase,type ServiceRefundCashPreparationScope} from "@erpx/server-finance/adapters/refund-purchases";
import {financeCommandReceipts} from "@erpx/server-workflow/public/finance-command-receipts";
import {financeOwnedSnapshotSources as commerceSnapshotSources} from "@erpx/server-commerce/public/finance-owned-snapshot-sources";
import { refundCashAccounts, refundCashAccountGuard, refundForeignCreditGuard } from '@erpx/server-commerce/public/finance-refund-source';
import { createRefunds } from '@erpx/server-finance/application/refunds';
import { refundApprovalSchema, refundInstructionSchema, refundExportSchema, refundConfirmationSchema, refundExceptionSchema, type Row } from '@erpx/server-finance/domain/refunds';
import { financeRefundProofFacts } from '@erpx/server-files/public/finance-refund-proof-sources';
import 'server-only';
import { nativeRefundNotification } from "./native-refund-notification";
import { ACTIVE_CHART_BASE_CURRENCY_SQL } from "./chart-base-currency-dedupe-sql";
import { z } from 'zod';
import { createHash } from 'node:crypto';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { ApiError } from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {prepareJournal} from '@erpx/server-finance/public/journal-planning';
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import {snapshotMoney} from '@erpx/server-finance/public/company-money';
export { refundApprovalSchema, refundInstructionSchema, refundExportSchema, refundConfirmationSchema, refundExceptionSchema } from '@erpx/server-finance/domain/refunds';
function scope(actor: WorkContext, alias = 'o') {
    return { sql: `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, args: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId] };
}
export async function getRefund(actor: WorkContext, id: string) { return withRefundPurchasedRead(actor,{type:"obligation",id},db=>refundService(db).getRefund(actor,id)); }
export async function listRefunds(actor: WorkContext, forWork = false) { return withFinancePurchasedRead(actor,db=>refundService(db).listRefunds(actor,forWork),"finance.banking"); }
export async function refundWorkspace(actor: WorkContext, id: string) { return withRefundBankEvidenceWorkspace(actor,id,async(db,bankScope)=>{const sourcePurchase=await refundSourcePurchase(db,actor,id),data=await refundService(db,{historyOnly:sourcePurchase?!sourcePurchase.settleExisting:false,sourceOnly:!!sourcePurchase&&sourcePurchase.capability!=="finance.banking"},undefined,bankScope).refundWorkspace(actor,id),bankEvidenceImportable=Object.fromEntries(await Promise.all(data.instructions.map(async instruction=>[String(instruction.id),await bankScope.canImport(data.owner,instruction)] as const)));return {...data,sourcePurchase,...(data.instructions.length?{bankEvidenceImportable}:{}),...(Object.keys(bankScope.functionalCurrencies).length?{bankEvidenceFunctionalCurrencies:{...bankScope.functionalCurrencies}}:{})};}); }
export async function approveRefund(actor: WorkContext, id: string, input: z.input<typeof refundApprovalSchema>, key: string) { return withRefundPurchasedCommand(actor,{type:"obligation",id},{key,operation:"finance.refund.approve"},tx=>refundService(tx).approveRefund(actor,id,input,key)); }
export async function prepareRefundInstruction(actor: WorkContext, id: string, input: z.input<typeof refundInstructionSchema>, key: string) { return withServiceRefundCashPreparation(actor,id,key,(tx,cashScope)=>refundService(tx,undefined,cashScope).prepareRefundInstruction(actor,id,input,key)); }
/** Immutable export receipt; exporting never moves cash or marks an obligation paid. */
export async function exportRefundInstructions(actor: WorkContext, input: z.input<typeof refundExportSchema>, key: string) { const parsed=refundExportSchema.parse(input);return withRefundPurchasedCommand(actor,{type:"instructions",ids:parsed.instructions.map(row=>row.id)},{key,operation:"finance.refund.export"},tx=>refundService(tx).exportRefundInstructions(actor,parsed,key)); }
export async function recordRefundException(actor: WorkContext, id: string, input: z.input<typeof refundExceptionSchema>, key: string) { return withRefundPurchasedCommand(actor,{type:"instructions",ids:[id]},{key,operation:"finance.refund.exception"},tx=>refundService(tx).recordRefundException(actor,id,input,key)); }
export async function confirmRefundInstruction(actor: WorkContext, id: string, input: z.input<typeof refundConfirmationSchema>, key: string) { return withRefundBankEvidenceConfirmation(actor,id,refundConfirmationSchema.parse(input),key,tx=>refundService(tx).confirmRefundInstruction(actor,id,input,key),()=>withRefundPurchasedCommand(actor,{type:"instructions",ids:[id]},{key,operation:"finance.refund.confirm"},tx=>refundService(tx).confirmRefundInstruction(actor,id,input,key))); }
export async function refundWork(actor: WorkContext): Promise<WorkItem[]> {
 const database=(await holdingResources(actor.holdingId)).write,access=await financeSourceAccess(database,actor,["finance.banking","finance.sales-posting","finance.inventory-valuation"]);
 if(!Object.values(access).some(value=>value.history))return [];
 const items=await refundService(database).refundWork(actor);
 return Promise.all(items.map(async item=>{const source=await refundSourcePurchase(database,actor,item.id);return !source||source.settleExisting?item:{...item,canAct:false,actionKind:"work" as const,queue:item.queue==="completed"?"completed" as const:"waiting" as const,nextAction:"Review refund history",relationships:["team" as const]};}));
}
export async function recoverRefundExport(actor: WorkContext, id: string) { return withRefundPurchasedRead(actor,{type:"instructions",ids:[id]},db=>refundService(db).recoverRefundExport(actor,id),true); }
function refundService(provided?:SqlDatabase,presentation?:{historyOnly:boolean;sourceOnly:boolean},cashScope?:ServiceRefundCashPreparationScope,bankScope?:RefundBankEvidenceReadScope) {
    let pending: Promise<SqlDatabase> | undefined=provided?Promise.resolve(provided):undefined;
    const db = (actor: WorkContext) => pending ??= holdingResources(actor.holdingId).then(r => r.write);
    const journals = new Map<string, Awaited<ReturnType<typeof prepareJournal>>>();
    let stagedMoney: Awaited<ReturnType<typeof snapshotMoney>>['statement'] | undefined, proofBinding: Awaited<ReturnType<typeof prepareAttachmentBinding>> | null = null;
    return createRefunds({ error: (...args) => new ApiError(...args), permission: requirePermission, ready: async (actor) => { await db(actor); }, now: Date.now, hashBytes: value => createHash('sha256').update(value, 'utf8').digest('hex'), replay: async <Result>(actor:WorkContext,key:string,operation:string,id:string,input:unknown) => replayCommand<Result>(await db(actor), actor, key, operation, id, input), obligation: async (actor, id) => { const s = scope(actor); return (await db(actor)).prepare(`SELECT o.*,COALESCE(r.requested_by,c.prepared_by) return_requested_by,COALESCE(r.return_date,c.entry_date) return_date,r.order_id,COALESCE(r.number,c.number) return_number,p.name party_name FROM finance_refund_obligations o LEFT ${commerceSnapshotSources().refunds_commerce_returns_r_join} ON r.id=o.return_id LEFT JOIN finance_billing_corrections c ON c.id=o.billing_correction_id AND c.company_id=o.company_id JOIN finance_parties p ON p.id=o.party_id WHERE o.id=? AND ${s.sql}`).bind(id, ...s.args).first<Row>(); }, instruction: async (actor, id) => (await db(actor)).prepare(REFUND_INSTRUCTION_POINT_SQL).bind(id, actor.companyId).first<Row>(), chart: async (actor, id) => (await db(actor)).prepare(ACTIVE_CHART_BASE_CURRENCY_SQL).bind(id, actor.companyId).first<{
            base_currency: string;
        }>(), fxAccounts: async (actor, chartId, currency, gainId, lossId) => (await (await db(actor)).prepare(ACCOUNT_SELECT_PREFIX + "company_id=? AND chart_id=? AND status='active' AND currency=? AND ((id=? AND category='other_income') OR (id=? AND category='other_expense'))").bind(actor.companyId, chartId, currency, gainId ?? '', lossId ?? '').all<Row>()).results, prepareMoney: async (actor, input) => { stagedMoney = (await snapshotMoney(await db(actor), actor, input)).statement; }, prepareJournal: async (actor, input, key, status) => { const journal = await prepareJournal(await db(actor), actor, input, key, status); journals.set(journal.result.id, journal); return { result: { id: journal.result.id, fxRateMicros: journal.result.fxRateMicros } }; }, prepareProof: async (actor, input) => { proofBinding = await prepareAttachmentBinding(await db(actor), actor, input); }, list: async (facts) => { const { forWork, actor } = facts; const write = await db(actor); const s = scope(actor),filtered=forWork&&await financePackageEnforcementMode()==="enforced",clock=filtered?`${materializedClockCteSql('refund_work_clock', packageClockSql, true)} `:"",purchase=filtered?` AND (SELECT ${financeRefundHistoryPredicate("o","refund_work_clock.now_ms")} FROM refund_work_clock)`:""; return (await write.prepare(`${clock}SELECT o.*,COALESCE(r.number,c.number) return_number,p.name party_name,r.order_id,COALESCE(r.requested_by,c.prepared_by) return_requested_by,(SELECT COUNT(*) FROM finance_refund_instructions i WHERE i.obligation_id=o.id AND i.status='unknown') uncertain_count,(SELECT COUNT(*) FROM finance_refund_instructions i WHERE i.obligation_id=o.id AND i.status IN ('exported','unknown') AND i.prepared_by<>?) reviewable_exceptions FROM finance_refund_obligations o LEFT ${commerceSnapshotSources().refunds_commerce_returns_r_join} ON r.id=o.return_id LEFT JOIN finance_billing_corrections c ON c.id=o.billing_correction_id AND c.company_id=o.company_id JOIN finance_parties p ON p.id=o.party_id WHERE ${s.sql}${purchase} ORDER BY o.updated_at DESC${forWork ? '' : ' LIMIT 200'}`).bind(actor.userId, ...s.args).all<Row>()).results; }, workspaceInstructions: async (facts) => { const { id, actor } = facts; const write = await db(actor); return (await write.prepare('SELECT * FROM finance_refund_instructions WHERE obligation_id=? ORDER BY created_at,id').bind(id).all<Row>()).results; }, cashAccounts: async (facts) => { const { actor, owner } = facts; const write = await db(actor); return await refundCashAccounts(write, actor, owner); }, openPeriods: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare(OPEN_PERIOD_LIST_ASC_SQL).bind(actor.companyId).all<Row>()).results; }, workspaceBank: async (facts) => { const { actor, owner } = facts; if(presentation?.historyOnly)return [];const write = await db(actor),cash=presentation?.sourceOnly?await refundCashAccounts(write,actor,owner):null;if(cash&&!cash.length)return [];const relevant=cash?` AND s.bank_account IN (${cash.map(()=>"?").join(",")}) AND t.amount_minor*? BETWEEN 1 AND ?`:"";const candidates=(await write.prepare(`SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.company_id=? AND t.currency=? AND t.match_status IN ('unmatched','proposed')${relevant} ORDER BY t.transaction_date DESC LIMIT 200`).bind(actor.companyId,owner.currency,...(cash?[...cash.map(account=>account.id),owner.direction==="outgoing"?-1:1,owner.outstanding_minor]:[])).all<Row>()).results;return bankScope?bankScope.candidates(owner,candidates):candidates; }, workspaceFx: async (facts) => { const { actor, owner } = facts; const write = await db(actor); return (await write.prepare("SELECT a.id,a.code,a.name,a.category FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.chart_id=? AND a.status='active' AND a.currency=c.base_currency AND a.category IN ('other_income','other_expense') ORDER BY a.code").bind(actor.companyId, owner.chart_id).all<Row>()).results; }, commitApproveRefund: async (facts) => {
            const { actor, key, input, id, now } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, 'finance.refund.approve', input, id, input.expectedVersion + 1, { id, status: 'approved', version: input.expectedVersion + 1 }, [
                assertion(write, key, 'obligation', idCompanyStatusVersionGuard("finance_refund_obligations", "pending"), [id, actor.companyId, input.expectedVersion]),
                await nativeFinanceRefunds(write, actor.companyId).obligationApprove([input.beneficiaryName, input.beneficiaryAccount, input.reason, actor.userId, now, now, id]),
                await nativeRefundNotification(write, actor, key, id, 'treasury', 'Refund approved: prepare and verify bank instructions', actor.userId),
            ]);
        }, commitPrepareRefundInstruction: async (facts) => {
            const { actor, key, commandInput, instructionId, id, input, amount, row, now } = facts;
            const write = await db(actor);
            const serviceCash=await cashScope?.capture(id,input.cashAccountId);
            const snapshot = { statement: stagedMoney! };
            return executeCommand(write, actor, key, 'finance.refund.prepare', commandInput, instructionId, 1, { id: instructionId, obligationId: id, status: 'prepared', version: 1 }, [
                assertion(write, key, 'available', "EXISTS(SELECT 1 FROM finance_refund_obligations o WHERE o.id=? AND o.company_id=? AND o.status='approved' AND o.version=? AND o.outstanding_minor-COALESCE((SELECT SUM(i.amount_minor) FROM finance_refund_instructions i WHERE i.obligation_id=o.id AND i.status IN ('prepared','exported','unknown')),0)>=?)", [id, actor.companyId, input.expectedVersion, amount]),
                refundCashAccountGuard(write, actor, key, { cashAccountId: input.cashAccountId, chartId: row.chart_id, balanceAccountId: row.balance_account_id, billingCorrectionId: row.billing_correction_id ?? null,serviceCash:serviceCash??undefined }),
                await nativeFinanceRefunds(write, actor.companyId).instructionInsert([instructionId, id, actor.companyId, amount, input.plannedDate, input.bankAccount, input.cashAccountId, input.fxGainAccountId ?? null, input.fxLossAccountId ?? null, actor.userId, now, now]),
                snapshot.statement,
                await nativeFinanceRefunds(write, actor.companyId).obligationBump([now, id]),
            ]);
        }, commitExportRefundInstructions: async (facts) => {
            const { actor, key, input, content, sha256, rows, now } = facts;
            const write = await db(actor);
            const exportSteps = await Promise.all(rows.map(async ({ row }) => [
                assertion(write, key, `instruction-${row.id}`, idCompanyStatusVersionGuard("finance_refund_instructions", "prepared"), [row.id, actor.companyId, row.version]),
                await nativeFinanceRefunds(write, actor.companyId).instructionExport([sha256, key, now, now, row.id]),
            ]));
            return executeCommand(write, actor, key, 'finance.refund.export', input, key, 1, { id: key, format: 'csv', content, sha256, rows: rows.length, confirmationRequired: true }, exportSteps.flat());
        }, commitRecordRefundException: async (facts) => {
            const { actor, key, input, id, row, now, owner } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, 'finance.refund.exception', input, id, input.expectedVersion + 1, { id, status: input.outcome, version: input.expectedVersion + 1 }, [
                assertion(write, key, 'instruction', 'EXISTS(SELECT 1 FROM finance_refund_instructions WHERE id=? AND company_id=? AND version=? AND status=?)', [id, actor.companyId, input.expectedVersion, row.status]),
                await nativeFinanceRefunds(write, actor.companyId).instructionResolve([input.outcome, input.evidence, actor.userId, now, id]),
                await nativeFinanceRefunds(write, actor.companyId).obligationBump2([now, owner.id]),
                await nativeRefundNotification(write, actor, key, String(owner.id), input.outcome === 'unknown' ? 'review' : 'treasury', 'Refund bank exception updated: review source evidence before retrying', actor.userId),
            ]);
        }, bank: async (facts) => { const { input, actor } = facts; const write = await db(actor); return write.prepare(BANK_TRANSACTION_BY_ID_SQL).bind(input.bankTransactionId, actor.companyId).first<Row>(); }, proofSets: async (facts) => { const { actor, input } = facts; const write = await db(actor); return (await financeRefundProofFacts(write).readRefundProofSets(actor.companyId, input.attachmentSetIds)).results; }, commitConfirmRefundInstruction: async (facts) => {
            const { actor, key, input, id, paymentId, journal: journalPlan, fxJournal: fxJournalPlan, carrying, cashBase, gain, loss, owner, amount, foreignSource, sourceBase, outgoing, row, bank, now } = facts;
            const write = await db(actor);
            const journal = journals.get(journalPlan.result.id)!, fxJournal = fxJournalPlan ? journals.get(fxJournalPlan.result.id)! : null, snapshot = { statement: stagedMoney! }, attachmentBinding = proofBinding;
            return executeCommand(write, actor, key, 'finance.refund.confirm', input, id, input.expectedVersion + 1, { id, status: 'confirmed', version: input.expectedVersion + 1, paymentId, journalId: journal.result.id, fxJournalId: fxJournal?.result.id ?? null, baseCarryingMinor: carrying, cashBaseMinor: cashBase, realizedFxMinor: gain - loss, bankTransactionId: input.bankTransactionId }, [
                ...(attachmentBinding?.guards ?? []),
                write.prepare('SELECT id FROM finance_refund_obligations WHERE id=? AND company_id=? FOR UPDATE').bind(owner.id, actor.companyId),
                assertion(write, key, 'instruction', "EXISTS(SELECT 1 FROM finance_refund_instructions WHERE id=? AND company_id=? AND version=? AND status IN ('exported','unknown'))", [id, actor.companyId, input.expectedVersion]),
                assertion(write, key, 'obligation', "EXISTS(SELECT 1 FROM finance_refund_obligations WHERE id=? AND company_id=? AND version=? AND status='approved' AND outstanding_minor>=?)", [owner.id, actor.companyId, owner.version, amount]),
                ...(foreignSource ? [refundForeignCreditGuard(write, key, owner.id, sourceBase)] : []),
                assertion(write, key, 'bank', "EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND t.amount_minor=? AND t.currency=? AND s.bank_account=?)", [input.bankTransactionId, actor.companyId, input.bankVersion, outgoing ? -amount : amount, owner.currency, row.bank_account]),
                ...journal.steps,
                ...(fxJournal?.steps ?? []),
                await nativeFinanceRefunds(write, actor.companyId).paymentInsert([paymentId, actor.companyId, owner.party_id, outgoing ? 'refund' : 'receipt', `RF-${key}`, bank.transaction_date, owner.currency, amount, row.bank_account, journal.result.id, journal.result.fxRateMicros, cashBase, fxJournal?.result.id ?? null, actor.userId, owner.approved_by, actor.userId, now, now]),
                snapshot.statement,
                await nativeFinanceRefunds(write, actor.companyId).bankMatch([paymentId, input.bankTransactionId]),
                await nativeFinanceRefunds(write, actor.companyId).instructionConfirm([input.bankReference, input.evidence, paymentId, input.bankTransactionId, actor.userId, carrying, cashBase, fxJournal?.result.id ?? null, now, id]),
                await nativeFinanceRefunds(write, actor.companyId).obligationSettle([amount, sourceBase, carrying, amount, now, owner.id]),
                ...(attachmentBinding?.steps ?? []),
                await nativeRefundNotification(write, actor, key, String(owner.id), 'completed', 'Refund bank evidence matched: review the remaining obligation', actor.userId),
            ]);
        }, exportReceipt: async (facts) => { const { row, actor } = facts; const write = await db(actor); return financeCommandReceipts(write).refundExport(row.export_command_id); } });
}
