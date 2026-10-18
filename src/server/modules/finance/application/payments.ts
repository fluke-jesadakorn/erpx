import { z } from 'zod';
import { confirmPaymentSchema, reversePaymentSchema, type FinanceRow, type JournalLineInput, type JournalInput } from '../domain/finance';
import { hasPermission } from "@/lib/identity_access/permissions";
import { samePerson } from "@/lib/identity_access/iam";
import { createFinancePolicy } from '../domain/finance-policy';
import { minorToDecimal } from "@/lib/finance/finance-money";
import type { JournalPlan } from './journal';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
export type PaymentCreateInput = {
    partyId?: string;
    kind: 'payment' | 'receipt' | 'refund';
    paymentNumber: string;
    paymentDate: string;
    currency: string;
    amount: string;
    bankAccount?: string;
    settlementInvoiceId?: string;
    correctionPaymentId?: string;
};
export type PaymentDecisionInput = {
    expectedVersion: number;
    proof: string;
};
export type PaymentConfirmation = z.infer<typeof confirmPaymentSchema>;
export type PaymentReversalInput = z.infer<typeof reversePaymentSchema>;
export type PaymentAllocationInput = {
    invoiceId: string;
    amount: string;
    expectedVersion: number;
};
export type PaymentBaseAllocation = {
    invoiceId: string;
    amount: number;
    invoice: FinanceRow;
    baseInvoice: number;
    baseSettlement: number;
    difference: number;
};
export interface FinancePaymentPorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    now(): number;
    id(): string;
    ready(actor: WorkContext): Promise<void>;
    settlementInvoice(actor: WorkContext, id: string): Promise<FinanceRow | null>;
    prior<Result=Record<string,unknown>>(actor: WorkContext, key: string, operation: string, input: unknown): Promise<Result|null>;
    replay<Result=Record<string,unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result|null>;
    moneyScale(companyId: string, currency: string): Promise<number>;
    digest(value: unknown): Promise<string>;
    prepareRequestedMoney(actor: WorkContext, id: string, input: PaymentCreateInput, currency: string, amount: number): Promise<void>;
    withholding(actor: WorkContext, payment: FinanceRow, key: string, date?: string, allocations?: PaymentConfirmation['allocations']): Promise<{
        source: {
            liabilityAccountId: string;
        };
    } | null>;
    prepareAttachments(actor: WorkContext, id: string, input: PaymentConfirmation, key: string): Promise<void>;
    functionalFx(actor: WorkContext, currency: string, baseCurrency: string, date: string): Promise<number>;
    prepareJournal(actor: WorkContext, input: JournalInput, key: string, status: 'posted'): Promise<JournalPlan>;
    prepareSettlementEvidence(actor: WorkContext, id: string, payment: FinanceRow, bank: FinanceRow, journal: JournalPlan, key: string, netBankMinor: number): Promise<void>;
    commitCreate(facts: {
        actor: WorkContext;
        input: PaymentCreateInput;
        id: string;
        paymentId: string;
        amount: number;
        currency: string;
        now: number;
        output: CommitCreateResult;
    }): Promise<CommitCreateResult>;
    commitApprove(facts: {
        actor: WorkContext;
        paymentId: string;
        input: PaymentDecisionInput;
        id: string;
        payment: FinanceRow;
        now: number;
        output: CommitApproveResult;
    }): Promise<CommitApproveResult>;
    commitRelease(facts: {
        actor: WorkContext;
        paymentId: string;
        input: PaymentDecisionInput;
        id: string;
        payment: FinanceRow;
        now: number;
        output: CommitReleaseResult;
    }): Promise<CommitReleaseResult>;
    commitConfirm(facts: {
        actor: WorkContext;
        paymentId: string;
        input: PaymentConfirmation;
        key: string;
        payment: FinanceRow;
        bank: FinanceRow;
        signed: number;
        byId: Map<string, FinanceRow>;
        amounts: number[];
        baseAllocations: PaymentBaseAllocation[];
        settlementRate: number;
        basePayment: number;
        journal: JournalPlan;
        fxJournal: JournalPlan | null;
        gain: number;
        loss: number;
        netBankMinor: number;
        now: number;
    }): Promise<CommitConfirmResult>;
    commitReverse(facts: {
        actor: WorkContext;
        paymentId: string;
        input: PaymentReversalInput;
        id: string;
        payment: FinanceRow;
        allocations: FinanceRow[];
        now: number;
        reversal: string;
        fxReversal: string | null;
    }): Promise<CommitReverseResult>;
    commitAllocation(facts: {
        actor: WorkContext;
        paymentId: string;
        input: PaymentAllocationInput;
        key: string;
        amount: number;
        now: number;
    }): Promise<CommitAllocationResult>;
    commitExport(facts: {
        actor: WorkContext;
        id: string;
        input: {
            paymentIds: string[];
        };
        output: CommitExportResult;
    }): Promise<CommitExportResult>;
    reviewPayment(facts: {
        paymentId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    releasedApprovedPayment(facts: {
        paymentId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    confirmationPayment(facts: {
        paymentId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    allocationInvoices(facts: {
        ids: string[];
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    confirmationBank(facts: {
        input: PaymentConfirmation;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    confirmationBankAccount(facts: {
        payment: FinanceRow;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    confirmationAccounts(facts: {
        accountIds: string[];
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    realizedFxAccounts(facts: {
        actor: WorkContext;
        bankAccount: FinanceRow;
        gain: number;
        input: PaymentConfirmation;
        loss: number;
    }): Promise<FinanceRow[]>;
    reversedPayment(facts: {
        paymentId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    reversalAllocations(facts: {
        paymentId: string;
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    creditedSource(facts: {
        paymentId: string;
        actor: WorkContext;
    }): Promise<boolean>;
    exportedPayments(facts: {
        paymentIds: string[];
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
}
export function createFinancePayments(ports: FinancePaymentPorts) {
    const { decimalToMinor, fxMicros, convertAtFx, proportionalBase, requireFinanceRole } = createFinancePolicy(ports.error);
    async function createPayment(actor: WorkContext, input: {
        partyId?: string;
        kind: "payment" | "receipt" | "refund";
        paymentNumber: string;
        paymentDate: string;
        currency: string;
        amount: string;
        bankAccount?: string;
        settlementInvoiceId?: string;
    correctionPaymentId?: string;
    }, id: string) {
        actor = ports.authorize(actor, "finance.write");
        if (actor.branchId || actor.departmentId)
            throw ports.error(403, "COMPANY_FINANCE_REQUIRED", "Use a Company finance assignment to prepare a controlled payment.");
        if (input.kind === "refund")
            throw ports.error(422, "REFUND_OWNER_WORKFLOW_REQUIRED", "Prepare refunds from the approved customer return or refund obligation.");
        if (!input.partyId || !input.bankAccount)
            throw ports.error(422, "PAYMENT_SETUP_REQUIRED", "Select the customer or supplier and bank ledger account.");
        await ports.ready(actor);
        const previous = await ports.prior<CommitCreateResult>(actor, id, "finance.payment.create", input);
        if (previous)
            return previous;
        const paymentId = `pay-${ports.id()}`;
        const amount = decimalToMinor(input.amount);
        if (amount <= 0)
            throw ports.error(422, "INVALID_PAYMENT_AMOUNT", "Payment amount must be greater than zero.");
        const currency = input.currency.toUpperCase(), now = ports.now();
        await ports.moneyScale(actor.companyId, currency);
        if (input.settlementInvoiceId) {
            const invoice = await ports.settlementInvoice(actor, input.settlementInvoiceId);
            if (!invoice || invoice.party_id !== input.partyId || invoice.currency !== currency || invoice.kind !== (input.kind === "receipt" ? "receivable" : "payable") || !["posted", "partially_paid"].includes(String(invoice.status)) || invoice.source_type === "expense_claim" || Number(invoice.outstanding_minor) < amount)
                throw ports.error(422, "PAYMENT_SETTLEMENT_SOURCE_INVALID", "Choose an open invoice for the same party, currency and payment amount.");
        }
        await ports.prepareRequestedMoney(actor, paymentId, input, currency, amount);
        const output = { id: paymentId, amount: minorToDecimal(amount), outstanding: minorToDecimal(amount), status: "draft", version: 1, ...(input.settlementInvoiceId ? { settlementInvoiceId: input.settlementInvoiceId } : {}), ...(input.correctionPaymentId ? {correctionPaymentId:input.correctionPaymentId} : {}) };
        return ports.commitCreate({ actor, input, id, paymentId, amount, currency, now, output });
    }
    async function approvePayment(actor: WorkContext, paymentId: string, input: {
        expectedVersion: number;
        proof: string;
    }, id: string) {
        actor = ports.authorize(actor, "finance.write");
        requireFinanceRole(actor, true);
        if (actor.branchId || actor.departmentId)
            throw ports.error(403, "COMPANY_FINANCE_REQUIRED", "Use a Company finance assignment to approve a controlled payment.");
        await ports.ready(actor);
        const previous = await ports.prior<CommitApproveResult>(actor, id, "finance.payment.approve", input);
        if (previous)
            return previous;
        if (!input.proof.trim())
            throw ports.error(422, "HUMAN_DECISION_REQUIRED", "Payment approval evidence is required.");
        const payment = await ports.reviewPayment({ paymentId, actor });
        if (!payment)
            throw ports.error(404, "PAYMENT_NOT_FOUND", "Payment not found.");
        if (Number(payment.version) !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "Payment changed. Refresh before approving.");
        if (samePerson(actor, payment.created_by))
            throw ports.error(409, "SELF_REVIEW_FORBIDDEN", "A different person must approve the payment.");
        if (payment.status !== "draft" && payment.status !== "in_review")
            throw ports.error(409, "PAYMENT_NOT_APPROVABLE", "Payment is not awaiting approval.");
        await ports.withholding(actor, payment, id);
        const now = ports.now();
        const output = { id: paymentId, status: "approved", version: input.expectedVersion + 1 };
        return ports.commitApprove({ actor, paymentId, input, id, payment, now, output });
    }
    async function releasePayment(actor: WorkContext, paymentId: string, input: {
        expectedVersion: number;
        proof: string;
    }, id: string) {
        actor = ports.authorize(actor, "finance.pay");
        if (actor.branchId || actor.departmentId)
            throw ports.error(403, "COMPANY_TREASURY_REQUIRED", "Use a Company treasury assignment to release a controlled payment.");
        await ports.ready(actor);
        const previous = await ports.prior<CommitReleaseResult>(actor, id, "finance.payment.release", input);
        if (previous)
            return previous;
        if (!input.proof.trim())
            throw ports.error(422, "HUMAN_DECISION_REQUIRED", "Payment release evidence is required.");
        const payment = await ports.releasedApprovedPayment({ paymentId, actor });
        if (!payment)
            throw ports.error(404, "PAYMENT_NOT_APPROVED", "Only an approved payment can be released.");
        if (Number(payment.version) !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "Payment changed. Refresh before releasing.");
        if (samePerson(actor, payment.created_by) || samePerson(actor, payment.approved_by))
            throw ports.error(403, "INDEPENDENT_PAYER_REQUIRED", "A third person must release and confirm this approved payment.");
        if (!payment.bank_account)
            throw ports.error(422, "BANK_ACCOUNT_REQUIRED", "Assign a bank ledger account before release.");
        await ports.withholding(actor, payment, id);
        const now = ports.now();
        const output = { id: paymentId, status: "released", version: input.expectedVersion + 1, bankEvidenceRequired: true };
        return ports.commitRelease({ actor, paymentId, input, id, payment, now, output });
    }
    async function confirmPayment(actor: WorkContext, paymentId: string, raw: z.input<typeof confirmPaymentSchema>, key: string) {
        actor = ports.authorize(actor, "finance.pay");
        if (actor.branchId || actor.departmentId)
            throw ports.error(403, "COMPANY_TREASURY_REQUIRED", "Use a Company treasury assignment to confirm bank evidence.");
        const input = confirmPaymentSchema.parse(raw);
        await ports.ready(actor);
        const previous = await ports.replay<CommitConfirmResult>(actor, key, "finance.payment.confirm", paymentId, input);
        if (previous)
            return previous;
        const payment = await ports.confirmationPayment({ paymentId, actor });
        if (!payment)
            throw ports.error(404, "PAYMENT_NOT_FOUND", "Payment not found.");
        if (payment.status !== "released" || Number(payment.version) !== input.expectedVersion)
            throw ports.error(409, "PAYMENT_NOT_CONFIRMABLE", "Refresh the released payment before confirming it.");
        if (payment.kind === "refund")
            throw ports.error(422, "REFUND_OWNER_WORKFLOW_REQUIRED", "Confirm refunds from their approved refund obligation.");
        if (!payment.party_id || !payment.bank_account)
            throw ports.error(422, "PAYMENT_SETUP_REQUIRED", "The payment needs a party and bank ledger account.");
        if (samePerson(actor, payment.created_by) || samePerson(actor, payment.approved_by) || !samePerson(actor, payment.released_by))
            throw ports.error(403, "INDEPENDENT_PAYER_REQUIRED", "The independent payer who released this payment must confirm its bank result.");
        if (input.attachmentSetIds.length)
            await ports.prepareAttachments(actor, paymentId, input, key);
        const ids = input.allocations.map(item => item.invoiceId);
        if (payment.settlement_invoice_id && (ids.length !== 1 || ids[0] !== payment.settlement_invoice_id))
            throw ports.error(422, "PAYMENT_SETTLEMENT_SOURCE_CHANGED", "Confirm this payment against its original invoice.");
        if (new Set(ids).size !== ids.length)
            throw ports.error(422, "DUPLICATE_ALLOCATION", "Each invoice may appear once.");
        const amounts = input.allocations.map(item => decimalToMinor(item.amount));
        if (amounts.some(amount => amount <= 0) || amounts.reduce((sum, amount) => sum + amount, 0) !== Number(payment.outstanding_minor))
            throw ports.error(422, "ALLOCATION_TOTAL_MISMATCH", "Allocation amounts must equal the payment amount.");
        const invoices = await ports.allocationInvoices({ ids, actor });
        if (invoices.length !== ids.length)
            throw ports.error(422, "INVOICE_INVALID", "Every selected invoice must belong to this company.");
        const byId = new Map(invoices.map(row => [String(row.id), row]));
        for (const [index, item] of input.allocations.entries()) {
            const invoice = byId.get(item.invoiceId)!;
            if (invoice.party_id !== payment.party_id || invoice.currency !== payment.currency || invoice.kind !== (payment.kind === "receipt" ? "receivable" : "payable") || !["posted", "partially_paid"].includes(String(invoice.status)) || invoice.source_type === "expense_claim" || Number(invoice.outstanding_minor) < amounts[index])
                throw ports.error(422, "INVOICE_ALLOCATION_MISMATCH", "Invoices must be open, compatible with the payment, and have enough outstanding balance.");
        }
        const netBankMinor = Number(payment.amount_minor) - Number(payment.withheld_minor ?? 0);
        const bank = await ports.confirmationBank({ input, actor }), signed = payment.kind === "receipt" ? Number(payment.amount_minor) : -netBankMinor;
        if (bank && (!["unmatched", "proposed"].includes(String(bank.match_status)) || Number(bank.version) !== input.bankVersion)) throw ports.error(409, "BANK_EVIDENCE_CHANGED", "The bank transaction changed or was claimed by another workflow. Refresh its evidence.");
        if (!bank || bank.bank_account !== payment.bank_account || bank.currency !== payment.currency || Number(bank.amount_minor) !== signed || String(bank.transaction_date) < String(payment.payment_date) || !["unmatched", "proposed"].includes(String(bank.match_status)) || Number(bank.version) !== input.bankVersion)
            throw ports.error(422, "BANK_EVIDENCE_MISMATCH", "Match the selected account, currency, date and exact bank amount.");
        const withholding = await ports.withholding(actor, payment, key, String(bank.transaction_date), input.allocations);
        const bankAccount = await ports.confirmationBankAccount({ payment, actor });
        if (!bankAccount)
            throw ports.error(422, "BANK_ACCOUNT_INVALID", "The selected ledger account is not configured as a bank control account.");
        const accountIds = [String(payment.bank_account), ...invoices.map(invoice => String(payment.kind === "receipt" ? invoice.debit_account_id : invoice.credit_account_id))], accounts = await ports.confirmationAccounts({ accountIds, actor });
        if (accounts.length !== new Set(accountIds).size || new Set(accounts.map(account => account.chart_id)).size !== 1 || accounts.some(account => account.currency !== payment.currency))
            throw ports.error(422, "PAYMENT_LEDGER_MISMATCH", "Bank and control accounts must use one active chart and currency.");
        const settlementRate = input.fxRate ? fxMicros(String(payment.currency), String(bankAccount.base_currency), input.fxRate) : await ports.functionalFx(actor, String(payment.currency), String(bankAccount.base_currency), String(bank.transaction_date)), basePayment = convertAtFx(netBankMinor, settlementRate);
        const baseAllocations = input.allocations.map((item, index) => { const invoice = byId.get(item.invoiceId)!, transactionOutstanding = Number(invoice.outstanding_minor), baseOutstanding = Number(invoice.base_outstanding_minor ?? convertAtFx(transactionOutstanding, Number(invoice.fx_rate_micros ?? 1000000))), baseInvoice = proportionalBase(baseOutstanding, transactionOutstanding, amounts[index]), baseSettlement = convertAtFx(amounts[index], settlementRate); return { ...item, amount: amounts[index], invoice, baseInvoice, baseSettlement, difference: baseSettlement - baseInvoice }; });
        const lines: JournalLineInput[] = input.allocations.map((item, index) => ({ accountId: String(payment.kind === "receipt" ? byId.get(item.invoiceId)!.debit_account_id : byId.get(item.invoiceId)!.credit_account_id), debit: payment.kind === "payment" ? minorToDecimal(amounts[index]) : "0", credit: payment.kind === "receipt" ? minorToDecimal(amounts[index]) : "0", description: `Allocate ${byId.get(item.invoiceId)!.invoice_number}` }));
        lines.push({ accountId: String(payment.bank_account), debit: payment.kind === "receipt" ? minorToDecimal(Number(payment.amount_minor)) : "0", credit: payment.kind === "payment" ? minorToDecimal(netBankMinor) : "0", description: input.bankReference });
        if (withholding && Number(payment.withheld_minor) > 0)
            lines.push({ accountId: withholding.source.liabilityAccountId, debit: "0", credit: minorToDecimal(Number(payment.withheld_minor)), description: "Independently reviewed Thai withholding tax" });
        const journal = await ports.prepareJournal(actor, { chartId: String(accounts[0].chart_id), periodId: input.periodId, entryDate: String(bank.transaction_date), currency: String(payment.currency), fxRate: input.fxRate, memo: `${payment.kind} ${payment.payment_number}: ${input.bankReference}`, sourceType: "payment", sourceId: paymentId, isOpening: false, lines }, key, "posted");
        const adjustmentLines: JournalLineInput[] = [], gain = baseAllocations.filter(row => payment.kind === "receipt" ? row.difference > 0 : row.difference < 0).reduce((sum, row) => sum + Math.abs(row.difference), 0), loss = baseAllocations.filter(row => payment.kind === "receipt" ? row.difference < 0 : row.difference > 0).reduce((sum, row) => sum + Math.abs(row.difference), 0);
        for (const row of baseAllocations) {
            if (!row.difference)
                continue;
            const debit = payment.kind === "receipt" ? row.difference > 0 : row.difference < 0;
            adjustmentLines.push({ accountId: String(payment.kind === "receipt" ? row.invoice.debit_account_id : row.invoice.credit_account_id), debit: debit ? minorToDecimal(Math.abs(row.difference)) : "0", credit: debit ? "0" : minorToDecimal(Math.abs(row.difference)), description: `Realized FX ${row.invoice.invoice_number}` });
        }
        if (gain) {
            if (!input.fxGainAccountId)
                throw ports.error(422, "FX_GAIN_ACCOUNT_REQUIRED", "Select the functional-currency realized FX gain account.");
            adjustmentLines.push({ accountId: input.fxGainAccountId, debit: "0", credit: minorToDecimal(gain), description: "Realized foreign exchange gain" });
        }
        if (loss) {
            if (!input.fxLossAccountId)
                throw ports.error(422, "FX_LOSS_ACCOUNT_REQUIRED", "Select the functional-currency realized FX loss account.");
            adjustmentLines.push({ accountId: input.fxLossAccountId, debit: minorToDecimal(loss), credit: "0", description: "Realized foreign exchange loss" });
        }
        if (gain || loss) {
            const required = [...(gain ? [input.fxGainAccountId!] : []), ...(loss ? [input.fxLossAccountId!] : [])], validFx = await ports.realizedFxAccounts({ actor, bankAccount, gain, input, loss });
            if (validFx.length !== new Set(required).size)
                throw ports.error(422, "FX_ACCOUNT_INVALID", "Realized gain and loss accounts must use the ledger functional currency and matching account category.");
        }
        const fxJournal = adjustmentLines.length ? await ports.prepareJournal(actor, { chartId: String(bankAccount.chart_id), periodId: input.periodId, entryDate: String(bank.transaction_date), currency: String(bankAccount.base_currency), memo: `Realized FX for ${payment.payment_number}`, sourceType: "payment_fx", sourceId: paymentId, isOpening: false, lines: adjustmentLines }, `${key}-fx`, "posted") : null, now = ports.now();
        await ports.prepareSettlementEvidence(actor, paymentId, payment, bank, journal, key, netBankMinor);
        return ports.commitConfirm({ actor, paymentId, input, key, payment, bank, signed, byId, amounts, baseAllocations, settlementRate, basePayment, journal, fxJournal, gain, loss, netBankMinor, now });
    }
    async function reversePayment(actor: WorkContext, paymentId: string, input: z.infer<typeof reversePaymentSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        requireFinanceRole(actor, true);
        await ports.ready(actor);
        const previous = await ports.replay<CommitReverseResult>(actor, id, "finance.payment.reverse", paymentId, input);
        if (previous)
            return previous;
        const payment = await ports.reversedPayment({ paymentId, actor });
        if (!payment)
            throw ports.error(404, "POSTED_PAYMENT_NOT_FOUND", "A posted payment with accounting evidence was not found.");
        if (!hasPermission(actor, "finance.approve") || samePerson(actor, payment.created_by))
            throw ports.error(403, "INDEPENDENT_CORRECTION_REQUIRED", "A different finance approver must reverse the payment.");
        if (payment.status !== "settled" || payment.journal_status !== "posted")
            throw ports.error(409, "PAYMENT_NOT_REVERSIBLE", "Only a settled payment with a posted journal can be reversed.");
        if (input.entryDate < String(payment.journal_entry_date))
            throw ports.error(422, "REVERSAL_BEFORE_SOURCE", "The reversal cannot precede the payment date.");
        const allocations = await ports.reversalAllocations({ paymentId, actor });
        if (!allocations.length)
            throw ports.error(409, "PAYMENT_ALLOCATION_REQUIRED", "Only an allocated payment can be corrected here.");
        if (await ports.creditedSource({ paymentId, actor }))
            throw ports.error(409, "PAYMENT_CREDITED_SOURCE_REVIEW", "Reconcile the credited bill and its refund before correcting the original receipt.");
        if (allocations.some(row => ["expense_claim", "pms"].includes(String(row.source_type))))
            throw ports.error(409, "REVERSE_AT_SOURCE", "Reverse this payment from its originating module.");
        const now = ports.now(), reversal = `payment-reversal-${id}`, fxReversal = payment.fx_journal_id ? `payment-fx-reversal-${id}` : null;
        for (const allocation of allocations) {
            const restored = Number(allocation.outstanding_minor) + Number(allocation.amount_minor);
            if (restored > Number(allocation.total_minor))
                throw ports.error(409, 'INVOICE_BALANCE_CONFLICT', 'The allocated invoice balance no longer permits this reversal.');
        }
        return ports.commitReverse({ actor, paymentId, input, id, payment, allocations, now, reversal, fxReversal });
    }
    async function allocatePayment(actor: WorkContext, paymentId: string, input: {
        invoiceId: string;
        amount: string;
        expectedVersion: number;
    }, key: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.replay<CommitAllocationResult>(actor, key, "finance.payment.allocate", paymentId, input);
        if (previous)
            return previous;
        const amount = decimalToMinor(input.amount);
        if (amount <= 0)
            throw ports.error(422, "INVALID_ALLOCATION", "Allocation must be positive.");
        return ports.commitAllocation({ actor, paymentId, input, key, amount, now: ports.now() });
    }
    async function exportPaymentFile(actor: WorkContext, paymentIds: string[], id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const input = { paymentIds };
        const previous = await ports.prior<CommitExportResult>(actor, id, "finance.payment.export", input);
        if (previous)
            return previous;
        if (!paymentIds.length)
            throw ports.error(422, "EMPTY_PAYMENT_BATCH", "Select at least one released payment.");
        const rows = await ports.exportedPayments({ paymentIds, actor });
        if (rows.length !== paymentIds.length)
            throw ports.error(409, "PAYMENT_BATCH_INVALID", "Every payment must be released and belong to this company.");
        const csv = ["payment_id,payment_number,payment_date,currency,amount,bank_account", ...rows.map((row) => [row.id, row.payment_number, row.payment_date, row.currency, minorToDecimal(Number(row.outstanding_minor) - Number(row.withheld_minor ?? 0)), row.bank_account ?? ""].map((value) => `"${String(value).replaceAll('"', '""')}"`).join(","))].join("\n");
        const output = { format: "csv", rows: rows.length, sha256: await ports.digest(csv), content: csv, settlementRequired: true };
        return ports.commitExport({ actor, id, input, output });
    }
    return { createPayment, approvePayment, releasePayment, confirmPayment, reversePayment, allocatePayment, exportPaymentFile };
}


export type CommitCreateResult = { id: string; amount: string; outstanding: string; status: string; version: number; settlementInvoiceId?: string; correctionPaymentId?: string; };
export type CommitApproveResult = { id: string; status: string; version: number; };
export type CommitReleaseResult = { id: string; status: string; version: number; bankEvidenceRequired: boolean; };
export type CommitConfirmResult = { id: string; status: string; version: number; journalId: string; fxJournalId: string | null; realizedFx: string; bankTransactionId: string | number | null; withheldMinor: number; netBankMinor: number; withholdingDocumentId: string | null; };
export type CommitReverseResult = { id: string; status: string; version: number; reversalJournalId: string; };
export type CommitAllocationResult = { paymentId: string; invoiceId: string; allocated: string; };
export type CommitExportResult = { format: string; rows: number; sha256: string; content: string; settlementRequired: boolean; };
