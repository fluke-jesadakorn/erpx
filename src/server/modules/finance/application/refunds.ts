import { z } from 'zod';
import { hasPermission } from "@/lib/identity_access/permissions";
import { samePerson } from "@/lib/identity_access/iam";
import { decimalToMinor, minorToDecimal } from "@/lib/finance/finance-money";
import type { WorkItem } from "@/lib/work_projection/work-item";
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
import type { JournalInput } from '../domain/finance';
import type { SnapshotInput } from '../domain/company-money';
import { refundApprovalSchema, refundInstructionSchema, refundExportSchema, refundConfirmationSchema, refundExceptionSchema, csvCell, createRefundPolicy, type Row, type RefundApproval, type RefundInstruction, type RefundExport, type RefundConfirmation, type RefundException, type RefundProof, type RefundJournal } from '../domain/refunds';
export type RefundProofBinding = {
    attachmentSetIds: string[];
    bindingContract: {
        targetType: "finance_refund_obligation";
        bindingType: "finance_refund_confirmation";
        contexts: Array<{
            purposeKey: "refund_proof";
            componentKind: "action";
            componentRef: string;
        }>;
    };
    targetId: string;
    expectedTargetVersion: number;
    commandId: string;
    bindingId: string;
};
export interface RefundPorts {
    error: FinanceError;
    permission(actor: WorkContext, permission: 'finance.read' | 'finance.approve' | 'finance.pay'): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    hashBytes(value: string): string;
    replay<Result=Record<string,unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result|null>;
    obligation(actor: WorkContext, id: string): Promise<Row | null>;
    instruction(actor: WorkContext, id: string): Promise<Row | null>;
    chart(actor: WorkContext, id: string): Promise<{
        base_currency: string;
    } | null>;
    fxAccounts(actor: WorkContext, chartId: string, currency: string, gainId: string | undefined, lossId: string | undefined): Promise<Row[]>;
    prepareMoney(actor: WorkContext, input: SnapshotInput): Promise<void>;
    prepareJournal(actor: WorkContext, input: JournalInput, key: string, status: 'posted'): Promise<RefundJournal>;
    prepareProof(actor: WorkContext, input: RefundProofBinding): Promise<void>;
    list(facts: {
        forWork: boolean;
        actor: WorkContext;
    }): Promise<Row[]>;
    workspaceInstructions(facts: {
        id: string;
        actor: WorkContext;
    }): Promise<Row[]>;
    cashAccounts(facts: {
        actor: WorkContext;
        owner: Row;
    }): Promise<Row[]>;
    openPeriods(facts: {
        actor: WorkContext;
    }): Promise<Row[]>;
    workspaceBank(facts: {
        actor: WorkContext;
        owner: Row;
    }): Promise<Row[]>;
    workspaceFx(facts: {
        actor: WorkContext;
        owner: Row;
    }): Promise<Row[]>;
    commitApproveRefund(facts: {
        actor: WorkContext;
        key: string;
        input: RefundApproval;
        id: string;
        now: number;
    }): Promise<CommitApproveRefundResult>;
    commitPrepareRefundInstruction(facts: {
        actor: WorkContext;
        key: string;
        commandInput: RefundInstruction & {
            obligationId: string;
        };
        instructionId: string;
        id: string;
        input: RefundInstruction;
        amount: number;
        row: Row;
        now: number;
    }): Promise<CommitPrepareRefundInstructionResult>;
    commitExportRefundInstructions(facts: {
        actor: WorkContext;
        key: string;
        input: RefundExport;
        content: string;
        sha256: string;
        rows: Array<{
            row: Row;
            owner: Row;
        }>;
        now: number;
    }): Promise<CommitExportRefundInstructionsResult>;
    commitRecordRefundException(facts: {
        actor: WorkContext;
        key: string;
        input: RefundException;
        id: string;
        row: Row;
        now: number;
        owner: Row;
    }): Promise<CommitRecordRefundExceptionResult>;
    bank(facts: {
        input: RefundConfirmation;
        actor: WorkContext;
    }): Promise<Row | null>;
    proofSets(facts: {
        actor: WorkContext;
        input: RefundConfirmation;
    }): Promise<RefundProof[]>;
    commitConfirmRefundInstruction(facts: {
        actor: WorkContext;
        key: string;
        input: RefundConfirmation;
        id: string;
        paymentId: string;
        journal: RefundJournal;
        fxJournal: RefundJournal | null;
        carrying: number;
        cashBase: number;
        gain: number;
        loss: number;
        owner: Row;
        amount: number;
        foreignSource: boolean;
        chart: {
            base_currency: string;
        };
        sourceBase: number;
        outgoing: boolean;
        row: Row;
        bank: Row;
        now: number;
    }): Promise<CommitConfirmRefundInstructionResult>;
    exportReceipt(facts: {
        row: Row;
        actor: WorkContext;
    }): Promise<{
        result_json: string;
    } | null>;
}
export function createRefunds(ports: RefundPorts) {
    const { checkVersion, proportionalCarrying } = createRefundPolicy(ports.error);
    function treasury(actor: WorkContext) {
        ports.permission(actor, 'finance.pay');
        if (actor.branchId || actor.departmentId)
            throw ports.error(403, 'COMPANY_TREASURY_REQUIRED', 'Use a Company treasury assignment to access bank evidence.');
    }
    function payer(actor: WorkContext, row: Row) {
        treasury(actor);
        if (samePerson(actor, row.approved_by) || samePerson(actor, row.created_by))
            throw ports.error(403, 'INDEPENDENT_PAYER_REQUIRED', 'A different treasury officer must execute the approved obligation.');
    }
    async function obligation(actor: WorkContext, id: string) {
        ports.permission(actor, 'finance.read');
        const row = await ports.obligation(actor, id);
        if (!row)
            throw ports.error(404, 'REFUND_NOT_FOUND', 'Refund obligation not found.');
        return row;
    }
    async function instruction(actor: WorkContext, id: string) {
        const row = await ports.instruction(actor, id);
        if (!row)
            throw ports.error(404, 'REFUND_INSTRUCTION_NOT_FOUND', 'Refund instruction not found.');
        return { row, owner: await obligation(actor, String(row.obligation_id)) };
    }
    async function fxAccounts(actor: WorkContext, chartId: string, gainId: string | undefined, lossId: string | undefined) {
        const chart = await ports.chart(actor, chartId);
        if (!chart)
            throw ports.error(404, 'CHART_NOT_FOUND', 'Active refund chart not found.');
        const expected = [...(gainId ? [gainId] : []), ...(lossId ? [lossId] : [])];
        if (expected.length) {
            const rows = await ports.fxAccounts(actor, chartId, chart.base_currency, gainId, lossId);
            if (rows.length !== new Set(expected).size)
                throw ports.error(422, 'FX_ACCOUNT_INVALID', 'Refund FX gain and loss accounts must use the functional currency and matching category.');
        }
        return chart;
    }
    async function getRefund(actor: WorkContext, id: string) { await ports.ready(actor); return obligation(actor, id); }
    async function listRefunds(actor: WorkContext, forWork = false) {
        ports.permission(actor, 'finance.read');
        await ports.ready(actor);
        return await ports.list({ forWork, actor });
    }
    async function refundWorkspace(actor: WorkContext, id: string) {
        await ports.ready(actor);
        const owner = await obligation(actor, id), bankAccess = hasPermission(actor, 'finance.pay') && !actor.branchId && !actor.departmentId;
        const [instructions, accounts, periods, bank, fx] = await Promise.all([
            ports.workspaceInstructions({ id, actor }),
            ports.cashAccounts({ actor, owner }),
            ports.openPeriods({ actor }),
            bankAccess ? ports.workspaceBank({ actor, owner }) : Promise.resolve([] as Row[]),
            ports.workspaceFx({ actor, owner }),
        ]);
        return { owner, instructions: instructions, accounts: accounts, periods: periods, bank: bank, fxAccounts: fx,
            canApprove: hasPermission(actor, 'finance.approve') && !samePerson(actor, owner.created_by) && !samePerson(actor, owner.return_requested_by),
            canPay: bankAccess && !samePerson(actor, owner.created_by) && !samePerson(actor, owner.approved_by), canReviewException: hasPermission(actor, 'finance.approve'), userId: actor.userId };
    }
    async function approveRefund(actor: WorkContext, id: string, raw: z.input<typeof refundApprovalSchema>, key: string) {
        const input = refundApprovalSchema.parse(raw);
        ports.permission(actor, 'finance.approve');
        await ports.ready(actor);
        const replay = await ports.replay<CommitApproveRefundResult>(actor, key, 'finance.refund.approve', id, input);
        if (replay)
            return replay;
        const row = await obligation(actor, id);
        checkVersion(row, input.expectedVersion);
        if (samePerson(actor, row.created_by) || samePerson(actor, row.return_requested_by))
            throw ports.error(403, 'INDEPENDENT_APPROVER_REQUIRED', 'The refund requires independent review of its source, beneficiary and amount.');
        const now = ports.now();
        return ports.commitApproveRefund({ actor, key, input, id, now });
    }
    async function prepareRefundInstruction(actor: WorkContext, id: string, raw: z.input<typeof refundInstructionSchema>, key: string) {
        const input = refundInstructionSchema.parse(raw);
        treasury(actor);
        await ports.ready(actor);
        const instructionId = `refund-instruction-${key}`, commandInput = { obligationId: id, ...input };
        const replay = await ports.replay<CommitPrepareRefundInstructionResult>(actor, key, 'finance.refund.prepare', instructionId, commandInput);
        if (replay)
            return replay;
        const row = await obligation(actor, id);
        payer(actor, row);
        checkVersion(row, input.expectedVersion);
        if (input.bankAccount !== input.cashAccountId)
            throw ports.error(422, 'BANK_LEDGER_MISMATCH', 'Select the same bank ledger account used by the imported statement.');
        const amount = decimalToMinor(input.amount);
        if (amount <= 0 || input.plannedDate < String(row.return_date))
            throw ports.error(422, 'INVALID_REFUND_AMOUNT_DATE', 'Use a positive amount and a date on or after the return.');
        await fxAccounts(actor, String(row.chart_id), input.fxGainAccountId, input.fxLossAccountId);
        await ports.prepareMoney(actor, { sourceType: 'finance_refund_instruction', sourceId: instructionId, amountKind: 'prepared', transactionDate: input.plannedDate, currency: String(row.currency), currencyScale: 2, amountUnits: String(amount) });
        const now = ports.now();
        return ports.commitPrepareRefundInstruction({ actor, key, commandInput, instructionId, id, input, amount, row, now });
    }
    async function exportRefundInstructions(actor: WorkContext, raw: z.input<typeof refundExportSchema>, key: string) {
        const input = refundExportSchema.parse(raw);
        treasury(actor);
        await ports.ready(actor);
        const replay = await ports.replay<CommitExportRefundInstructionsResult>(actor, key, 'finance.refund.export', key, input);
        if (replay)
            return replay;
        if (new Set(input.instructions.map(i => i.id)).size !== input.instructions.length)
            throw ports.error(422, 'DUPLICATE_INSTRUCTION', 'Select each instruction once.');
        const rows = [];
        for (const item of input.instructions) {
            const { row, owner } = await instruction(actor, item.id);
            payer(actor, owner);
            checkVersion(row, item.expectedVersion);
            if (row.status !== 'prepared' || owner.status !== 'approved')
                throw ports.error(409, 'REFUND_NOT_PREPARED', 'Only prepared instructions for approved obligations can be exported.');
            rows.push({ row, owner });
        }
        const content = [['instruction_id', 'source_return', 'direction', 'date', 'currency', 'amount', 'company_bank_account', 'counterparty', 'counterparty_account'].map(csvCell).join(','), ...rows.map(({ row, owner }) => [row.id, owner.return_number, owner.direction, row.planned_date, owner.currency, minorToDecimal(Number(row.amount_minor)), row.bank_account, owner.beneficiary_name, owner.beneficiary_account].map(csvCell).join(','))].join('\r\n');
        const sha256 = ports.hashBytes(content), now = ports.now();
        return ports.commitExportRefundInstructions({ actor, key, input, content, sha256, rows, now });
    }
    async function recordRefundException(actor: WorkContext, id: string, raw: z.input<typeof refundExceptionSchema>, key: string) {
        const input = refundExceptionSchema.parse(raw);
        ports.permission(actor, 'finance.read');
        await ports.ready(actor);
        const replay = await ports.replay<CommitRecordRefundExceptionResult>(actor, key, 'finance.refund.exception', id, input);
        if (replay)
            return replay;
        const { row, owner } = await instruction(actor, id);
        checkVersion(row, input.expectedVersion);
        if (input.outcome === 'cancelled') {
            payer(actor, owner);
            if (row.status !== 'prepared')
                throw ports.error(409, 'EXPORTED_REFUND_REQUIRES_REVIEW', 'An exported instruction cannot be cancelled without reviewing the bank result.');
        }
        else if (input.outcome === 'unknown') {
            payer(actor, owner);
            if (row.status !== 'exported')
                throw ports.error(409, 'REFUND_NOT_EXPORTED', 'Only exported instructions can have an uncertain bank result.');
        }
        else {
            ports.permission(actor, 'finance.approve');
            if (samePerson(actor, row.prepared_by))
                throw ports.error(403, 'INDEPENDENT_FAILURE_REVIEW_REQUIRED', 'A different finance approver must verify definitive bank failure.');
            if (!['exported', 'unknown'].includes(String(row.status)))
                throw ports.error(409, 'REFUND_NOT_EXPORTED', 'Review an exported or uncertain instruction.');
        }
        const now = ports.now();
        return ports.commitRecordRefundException({ actor, key, input, id, row, now, owner });
    }
    async function confirmRefundInstruction(actor: WorkContext, id: string, raw: z.input<typeof refundConfirmationSchema>, key: string) {
        const input = refundConfirmationSchema.parse(raw);
        treasury(actor);
        await ports.ready(actor);
        const replay = await ports.replay<CommitConfirmRefundInstructionResult>(actor, key, 'finance.refund.confirm', id, input);
        if (replay)
            return replay;
        const { row, owner } = await instruction(actor, id);
        payer(actor, owner);
        checkVersion(row, input.expectedVersion);
        if (!['exported', 'unknown'].includes(String(row.status)) || owner.status !== 'approved')
            throw ports.error(409, 'REFUND_NOT_EXPORTED', 'Confirm an approved exported instruction using bank evidence.');
        const bank = await ports.bank({ input, actor });
        const outgoing = owner.direction === 'outgoing', amount = Number(row.amount_minor);
        if (!bank || bank.currency !== owner.currency || bank.bank_account !== row.bank_account || Number(bank.amount_minor) !== (outgoing ? -amount : amount) || String(bank.transaction_date) < String(row.planned_date))
            throw ports.error(422, 'BANK_EVIDENCE_MISMATCH', 'Bank account, currency, direction, amount and payment date must match the instruction.');
        if (input.attachmentSetIds.length) {
            const sets = await ports.proofSets({ actor, input });
            if (sets.length !== new Set(input.attachmentSetIds).size || sets.some(set => set.purpose_key !== 'refund_proof' || set.component_kind !== 'action' || set.component_ref !== `refund-instruction:${id}`))
                throw ports.error(422, 'REFUND_ATTACHMENT_PURPOSE', 'Choose only refund proof prepared for this bank confirmation.');
            await ports.prepareProof(actor, { attachmentSetIds: input.attachmentSetIds, bindingContract: { targetType: 'finance_refund_obligation', bindingType: 'finance_refund_confirmation', contexts: [{ purposeKey: 'refund_proof', componentKind: 'action', componentRef: `refund-instruction:${id}` }] }, targetId: String(owner.id), expectedTargetVersion: Number(owner.version), commandId: key, bindingId: id });
        }
        const journal = await ports.prepareJournal(actor, { chartId: String(owner.chart_id), periodId: input.periodId, entryDate: String(bank.transaction_date), currency: String(owner.currency), memo: `Refund ${owner.return_number} · ${input.bankReference}`, sourceType: 'return_refund', sourceId: id, isOpening: false, lines: [
                { accountId: String(owner.balance_account_id), debit: outgoing ? minorToDecimal(amount) : '0', credit: outgoing ? '0' : minorToDecimal(amount) },
                { accountId: String(row.cash_account_id), debit: outgoing ? '0' : minorToDecimal(amount), credit: outgoing ? minorToDecimal(amount) : '0' },
            ] }, key, 'posted');
        const chart = await fxAccounts(actor, String(owner.chart_id), row.fx_gain_account_id ? String(row.fx_gain_account_id) : undefined, row.fx_loss_account_id ? String(row.fx_loss_account_id) : undefined);
        // Legacy functional-currency obligations keep their original one-to-one
        // meaning. Foreign obligations require the owner's retained credit evidence.
        const sourceBase = owner.base_outstanding_minor === null || owner.base_outstanding_minor === undefined
            ? owner.currency === chart.base_currency ? Number(owner.outstanding_minor) : null : Number(owner.base_outstanding_minor);
        if (sourceBase === null)
            throw ports.error(409, 'REFUND_CARRYING_EVIDENCE_REQUIRED', 'Reconcile the foreign refund source carrying value before confirming cash.');
        const carrying = proportionalCarrying(sourceBase, Number(owner.outstanding_minor), amount), cashBase = Number((BigInt(amount) * BigInt(journal.result.fxRateMicros) + BigInt(500000)) / BigInt(1000000));
        if (!Number.isSafeInteger(cashBase))
            throw ports.error(422, 'MONEY_OVERFLOW', 'Refund cash exceeds supported precision.');
        const difference = cashBase - carrying, gain = (outgoing ? difference < 0 : difference > 0) ? Math.abs(difference) : 0, loss = (outgoing ? difference > 0 : difference < 0) ? Math.abs(difference) : 0;
        const fxAccount = gain ? row.fx_gain_account_id : loss ? row.fx_loss_account_id : null;
        if ((gain || loss) && !fxAccount)
            throw ports.error(422, gain ? 'FX_GAIN_ACCOUNT_REQUIRED' : 'FX_LOSS_ACCOUNT_REQUIRED', 'Prepare the refund instruction with the required functional-currency realized FX account.');
        const controlDebit = outgoing ? difference < 0 : difference > 0;
        const fxJournal = difference ? await ports.prepareJournal(actor, { chartId: String(owner.chart_id), periodId: input.periodId, entryDate: String(bank.transaction_date), currency: chart.base_currency, memo: `Realized FX for refund ${owner.return_number}`, sourceType: 'return_refund_fx', sourceId: id, isOpening: false, lines: [
                { accountId: String(owner.balance_account_id), debit: controlDebit ? minorToDecimal(Math.abs(difference)) : '0', credit: controlDebit ? '0' : minorToDecimal(Math.abs(difference)) },
                { accountId: String(fxAccount), debit: loss ? minorToDecimal(loss) : '0', credit: gain ? minorToDecimal(gain) : '0' },
            ] }, `${key}-fx`, 'posted') : null;
        const paymentId = `refund-payment-${key}`, now = ports.now();
        await ports.prepareMoney(actor, { sourceType: 'finance_payment', sourceId: paymentId, amountKind: 'settled', transactionDate: String(bank.transaction_date), currency: String(owner.currency), currencyScale: 2, amountUnits: String(amount) });
        return ports.commitConfirmRefundInstruction({ actor, key, input, id, paymentId, journal, fxJournal, carrying, cashBase, gain, loss, owner, amount, foreignSource: owner.currency !== chart.base_currency, chart, sourceBase, outgoing, row, bank, now });
    }
    async function refundWork(actor: WorkContext): Promise<WorkItem[]> {
        if (!hasPermission(actor, 'finance.read'))
            return [];
        const rows = await listRefunds(actor, true);
        return rows.map(row => {
            const approve = row.status === 'pending' && hasPermission(actor, 'finance.approve') && !samePerson(actor, row.created_by) && !samePerson(actor, row.return_requested_by);
            const pay = row.status === 'approved' && hasPermission(actor, 'finance.pay') && !actor.branchId && !actor.departmentId && !samePerson(actor, row.approved_by) && !samePerson(actor, row.created_by);
            const reviewException = Number(row.reviewable_exceptions) > 0 && hasPermission(actor, 'finance.approve');
            const canAct = approve || pay || reviewException;
            return { id: String(row.id), type: 'refund', actionKind: approve || reviewException ? 'approval' : 'work', title: `Refund · ${row.return_number}`, detail: `${row.party_name} · ${row.currency} ${minorToDecimal(Number(row.outstanding_minor))} remaining${Number(row.uncertain_count) > 0 ? ' · Bank result uncertain' : ''}`, href: `/finance/refunds/${row.id}`, status: String(row.status), updatedAt: Number(row.updated_at), queue: row.status === 'settled' ? 'completed' : canAct ? 'todo' : 'waiting', canAct, responsibleQueue: row.status === 'pending' || reviewException ? 'Independent Finance approver' : 'Company Treasury', nextAction: row.status === 'pending' ? 'Review source, amount and beneficiary' : reviewException ? 'Review bank result before releasing a failed instruction' : 'Review export and bank confirmation', relationships: [canAct ? 'approval' : 'team'] };
        });
    }
    async function recoverRefundExport(actor: WorkContext, id: string) {
        treasury(actor);
        await ports.ready(actor);
        const { row, owner } = await instruction(actor, id);
        payer(actor, owner);
        const receipt = await ports.exportReceipt({ row, actor });
        if (!receipt)
            throw ports.error(404, 'REFUND_EXPORT_NOT_FOUND', 'No export receipt exists for this instruction.');
        return JSON.parse(receipt.result_json) as {
            content: string;
            sha256: string;
        };
    }
    return { getRefund, listRefunds, refundWorkspace, approveRefund, prepareRefundInstruction, exportRefundInstructions, recordRefundException, confirmRefundInstruction, refundWork, recoverRefundExport };
}


export type CommitApproveRefundResult = { id: string; status: string; version: number; };
export type CommitPrepareRefundInstructionResult = { id: string; obligationId: string; status: string; version: number; };
export type CommitExportRefundInstructionsResult = { id: string; format: string; content: string; sha256: string; rows: number; confirmationRequired: true; };
export type CommitRecordRefundExceptionResult = { id: string; status: "unknown" | "failed" | "cancelled"; version: number; };
export type CommitConfirmRefundInstructionResult = { id: string; status: string; version: number; paymentId: string; journalId: string; fxJournalId: string | null; baseCarryingMinor: number; cashBaseMinor: number; realizedFxMinor: number; bankTransactionId: string; };
