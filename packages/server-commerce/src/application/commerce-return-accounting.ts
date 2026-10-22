import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { Clock } from "@erpx/server-shared/application/ports";
import type { AttachmentManifestItem } from "@erpx/server-files/public/attachments/types";
import type { FunctionalFxEvidence } from "@erpx/server-finance/public/commerce-journal-contracts";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
export type Row = Record<string, string | number | null>;
export type CommerceRows<Row> = {
    results: Row[];
};
export type CommerceAttachmentBinding<Effect> = {
    manifest: AttachmentManifestItem[];
    guards: Effect[];
    steps: Effect[];
};
export type CommerceJournal<Effect> = {
    steps: Effect[];
    guards: Effect[];
    preparedLines: Array<{
        debit: number;
        credit: number;
        baseDebit: number;
        baseCredit: number;
        accountId: string;
        description?: string;
        dimensions?: Record<string, string>;
    }>;
    preparedFxEvidence: FunctionalFxEvidence | null;
    result: {
        id: string;
        number: string;
        status: "draft" | "posted";
        version: number;
        debit: string;
        credit: string;
        fxRateMicros: number;
        baseCurrency: string;
    };
};
export type CommerceReturnsUnit<Effect> = {
    readFinanceJournalLines1(facts: {
        invoiceId: string;
        companyId: string;
    }): Promise<{
        id: string;
        company_id: string;
        kind: string;
        currency: string;
        total_minor: number;
        fx_rate_micros: number;
        debit_account_id: string;
        credit_account_id: string;
        journal_id: string;
        chart_id: string;
        status: string;
        base_currency: string;
        control_base: number;
    } | null>;
    readFinanceJournalLines2(facts: {
        incoming: boolean;
        control: string;
        id: string;
        returnId: string;
    }): Promise<{
        transaction_total: number;
        base_total: number;
    } | null>;
    readFinanceJournalLines3(facts: {
        journal_id: string;
    }): Promise<CommerceRows<{
        account_id: string;
        transaction_amount: number;
        base_amount: number;
    }>>;
    readCommerceReturns1(facts: {
        id: string;
        returnId: string;
    }): Promise<CommerceRows<{
        account_id: string;
        transaction_amount: number;
        base_amount: number;
    }>>;
    prepareFinanceInvoices1(facts: {
        id: string;
    }): Effect;
    prepareFinanceJournalLines1(facts: {
        baseDebit: number;
        baseCredit: number;
        id: string;
        ordinal: number;
        accountId: string;
    }): Effect;
    guardRule1(facts: {
        key: string;
        guardId: string;
        values: (string | number)[];
    }): Effect;
    guardRule2(facts: {
        key: string;
        guardId: string;
        values: string[];
    }): Effect;
    prepareJournal(arg0: WorkContext, arg1: {
        chartId: string;
        periodId: string;
        entryDate: string;
        currency: string;
        fxRate: string;
        memo: string;
        sourceType: string;
        sourceId: string;
        isOpening: false;
        lines: {
            accountId: string;
            debit: string;
            credit: string;
        }[];
    }, arg2: string, arg3: "draft"): Promise<{
        steps: Effect[];
        guards: Effect[];
        preparedLines: {
            debit: number;
            credit: number;
            baseDebit: number;
            baseCredit: number;
            accountId: string;
            description?: string | undefined;
            dimensions?: Record<string, string> | undefined;
        }[];
        preparedFxEvidence: FunctionalFxEvidence | null;
        result: {
            id: string;
            number: string;
            status: "draft" | "posted";
            version: number;
            debit: string;
            credit: string;
            fxRateMicros: number;
            baseCurrency: string;
        };
    }>;
};
export type Posting = {
    account: string;
    debit: number;
    credit: number;
};
export function createCommerceReturnsApplication<Effect, Context = unknown>(ports: {
    clock: Clock;
    require(actor: WorkContext, permission: string): WorkContext;
    error(status: number, code: string, message: string): Error;
    hash(value: string): Promise<string>;
    forActor(actor: WorkContext): Promise<CommerceReturnsUnit<Effect>>;
    forContext(context: Context): Promise<CommerceReturnsUnit<Effect>>;
    attachmentReadable(): boolean;
    returnCompletionValues(input: {
        totalMinor: number;
        baseCreditMinor: number;
        outstandingMinor: number;
        baseOutstandingMinor: number;
    }): {
        creditedMinor: number;
        refundMinor: number;
        baseCreditedMinor: number;
        baseRefundMinor: number;
        realizedFxMinor: number;
    };
}) {
    function proportional(total: number, numerator: number, denominator: number) {
        if (denominator <= 0 || numerator < 0 || numerator > denominator)
            throw ports.error(409, 'RETURN_CARRYING_RECONCILIATION_REQUIRED', 'Reconcile original return carrying evidence before posting.');
        return Number((BigInt(total) * BigInt(numerator) + BigInt(Math.floor(denominator / 2))) / BigInt(denominator));
    }
    /** The return consumes the original posted control value, including its final rounding cent. */
    async function prepareReturnCredit(write: CommerceReturnsUnit<Effect>, a: WorkContext, input: {
        key: string;
        invoiceId: string;
        returnId: string;
        periodId: string;
        entryDate: string;
        postings: Posting[];
    }) {
        const original = await write.readFinanceJournalLines1({ invoiceId: input.invoiceId, companyId: a.companyId });
        if (!original || original.status !== 'posted' || original.debit_account_id === original.credit_account_id || Number(original.control_base) <= 0)
            throw ports.error(409, 'RETURN_ORIGINAL_CONTROL_REQUIRED', 'The original posted invoice control value must be reconciled before crediting.');
        const incoming = original.kind === 'receivable', control = incoming ? original.debit_account_id : original.credit_account_id, total = input.postings.find(p => p.account === control)?.[incoming ? 'credit' : 'debit'];
        if (total === undefined || total <= 0)
            throw ports.error(422, 'RETURN_CONTROL_MAPPING_REQUIRED', 'The credit note must reverse the original invoice control account.');
        const prior = await write.readFinanceJournalLines2({ incoming: incoming, control: control, id: original.id, returnId: input.returnId });
        const previous = Number(prior?.transaction_total ?? 0), previousBase = Number(prior?.base_total ?? 0), baseCredit = proportional(Number(original.control_base), previous + total, Number(original.total_minor)) - previousBase;
        if (baseCredit < 0)
            throw ports.error(409, 'RETURN_CARRYING_RECONCILIATION_REQUIRED', 'Earlier credits do not reconcile to the original invoice value.');
        const journal = await write.prepareJournal(a, { chartId: original.chart_id, periodId: input.periodId, entryDate: input.entryDate, currency: original.currency,
            fxRate: minorToDecimal(Number(original.fx_rate_micros), 6), memo: `Original carrying-value return ${input.returnId}`, sourceType: incoming ? 'credit_note' : 'debit_note', sourceId: input.returnId, isOpening: false,
            lines: input.postings.filter(p => p.debit || p.credit).map(p => ({ accountId: p.account, debit: minorToDecimal(p.debit), credit: minorToDecimal(p.credit) })) }, `${input.key}:credit`, 'draft');
        // Allocate each retained original account, including VAT and reviewed price
        // variance. A full return must reverse every original account's final cent.
        // Adjust a partial account's cent only while that source still has capacity;
        // never move a fully consumed tax amount onto another account.
        const origin = (await write.readFinanceJournalLines3({ journal_id: original.journal_id })).results;
        const earlier = (await write.readCommerceReturns1({ id: original.id, returnId: input.returnId })).results;
        const allocated = journal.preparedLines.map(line => {
            const source = origin.find(row => row.account_id === line.accountId), used = earlier.find(row => row.account_id === line.accountId), transaction = line.credit - line.debit, sourceAmount = Number(source?.transaction_amount ?? 0), previousAmount = Number(used?.transaction_amount ?? 0), previousCarrying = Number(used?.base_amount ?? 0), cumulative = previousAmount + transaction, baseSource = Number(source?.base_amount ?? 0);
            if (!source || !sourceAmount || Math.sign(transaction) !== Math.sign(sourceAmount) || Math.sign(cumulative) !== Math.sign(sourceAmount) || Math.abs(cumulative) > Math.abs(sourceAmount))
                throw ports.error(409, 'RETURN_ACCOUNT_CARRYING_REQUIRED', 'The credit must consume the original posted account and document amount.');
            const carrying = Math.sign(baseSource) * (proportional(Math.abs(baseSource), Math.abs(cumulative), Math.abs(sourceAmount))) - previousCarrying;
            if (carrying && Math.sign(carrying) !== Math.sign(sourceAmount))
                throw ports.error(409, 'RETURN_CARRYING_RECONCILIATION_REQUIRED', 'Earlier account credits do not reconcile to the original posted source.');
            return { ...line, baseDebit: line.debit ? Math.abs(carrying) : 0, baseCredit: line.credit ? Math.abs(carrying) : 0,
                fullyConsumed: Math.abs(cumulative) === Math.abs(sourceAmount), remainingBase: Math.abs(baseSource) - Math.abs(previousCarrying) - Math.abs(carrying) };
        });
        const controlPosting = allocated.find(line => line.accountId === control);
        if (!controlPosting || (incoming ? controlPosting.baseCredit : controlPosting.baseDebit) !== baseCredit)
            throw ports.error(409, 'RETURN_CARRYING_RECONCILIATION_REQUIRED', 'Original control and retained account credits must agree.');
        const difference = allocated.reduce((sum, line) => sum + line.baseDebit - line.baseCredit, 0);
        if (difference) {
            const candidates = allocated.filter(line => line.accountId !== control && !line.fullyConsumed)
                .sort((left, right) => right.remainingBase - left.remainingBase || right.baseDebit + right.baseCredit - left.baseDebit - left.baseCredit);
            const counter = candidates.find(line => { const delta = line.credit ? difference : -difference, amount = line.baseDebit + line.baseCredit + delta; return amount >= 0 && delta <= line.remainingBase; });
            if (!counter)
                throw ports.error(409, 'RETURN_CARRYING_RECONCILIATION_REQUIRED', 'Partial account rounding cannot consume more than the original source carrying.');
            if (counter.credit)
                counter.baseCredit += difference;
            else
                counter.baseDebit -= difference;
        }
        const basePostings = allocated.map((line, index) => ({ ...journal.preparedLines[index], baseDebit: line.baseDebit, baseCredit: line.baseCredit }));
        const steps: Effect[] = [
            write.prepareFinanceInvoices1({ id: original.id }),
            write.guardRule1({ key: input.key, guardId: 'original-credit-capacity', values: [original.id, input.returnId, previous] }),
            ...journal.steps,
            ...basePostings.map((line, ordinal) => write.prepareFinanceJournalLines1({ baseDebit: line.baseDebit, baseCredit: line.baseCredit, id: journal.result.id, ordinal: ordinal, accountId: line.accountId })),
            write.guardRule2({ key: input.key, guardId: 'credit-carrying-balance', values: [journal.result.id] }),
        ];
        return { steps, journalId: journal.result.id, baseCreditMinor: baseCredit, controlAccountId: control, baseCurrency: original.base_currency, basePostings };
    }
    /** Separate applied receivable/payable carrying from the refund that stays on the control account. */
    function returnCompletionValues(input: {
        totalMinor: number;
        baseCreditMinor: number;
        outstandingMinor: number;
        baseOutstandingMinor: number;
    }) {
        const credited = Math.min(input.totalMinor, input.outstandingMinor), refund = input.totalMinor - credited, originalApplied = credited === input.totalMinor ? input.baseCreditMinor : proportional(input.baseCreditMinor, credited, input.totalMinor), currentApplied = credited ? proportional(input.baseOutstandingMinor, credited, input.outstandingMinor) : 0;
        return { creditedMinor: credited, refundMinor: refund, baseCreditedMinor: currentApplied, baseRefundMinor: input.baseCreditMinor - originalApplied, realizedFxMinor: currentApplied - originalApplied };
    }
    return { prepareReturnCredit, returnCompletionValues };
}
