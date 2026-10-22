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
export type CommerceGoodsUnit<Effect> = {
    readFinanceAccounts1(facts: {
        id: string;
        companyId: string;
        chartId: string;
        currency: string;
        category: "other_income" | "other_expense";
    }): Promise<Record<string, unknown> | null>;
    readFinanceAccounts2(facts: {
        debit_account_id: string | number | null;
        companyId: string;
    }): Promise<{
        chart_id: string;
    } | null>;
    readFinanceJournalLines1(facts: {
        debit_account_id: string | number | null;
        debit_account_id2: string | number | null;
        id: string | number | null;
        companyId: string;
        chartId: string;
        branchId: string | null;
        branchId2: string | null;
        departmentId: string | null;
        departmentId2: string | null;
    }): Promise<CommerceRows<Row>>;
    prepareCommerceFulfillments1(facts: {
        id: string | number | null;
    }): Effect;
    prepareCommerceInvoiceGoodsCarrying1(facts: {
        id: string | number | null;
        companyId: string;
        chartId: string;
        currency: string;
        debit_account_id: string | number | null;
        fxRateMicros: number;
        fact6: "confirmed_company_quote" | "reviewed_input";
        fact7: string | null;
        fact8: string | null;
        fact9: boolean;
        fact10: string;
        sourceJson: string;
        receiptBase: number;
        invoiceBase: number;
        difference: number;
        fact15: string | null;
        fact16: string | null;
        fact17: string | null;
        userId: string;
        fact19: number;
    }): Effect;
    readCommerceInvoiceGoodsCarrying1(facts: {
        invoiceId: string;
        companyId: string;
    }): Promise<Row | null>;
    guardRule1(facts: {
        key: string;
        guardId: string;
        values: (string | number | null)[];
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
        isOpening: false;
        sourceType: string;
        sourceId: string;
        memo: string;
        lines: {
            accountId: string;
            debit: string;
            credit: string;
        }[];
    } | {
        chartId: string;
        periodId: string;
        entryDate: string;
        currency: string;
        sourceType: string;
        sourceId: string;
        memo: string;
        isOpening: false;
        lines: {
            accountId: string;
            debit: string;
            credit: string;
        }[];
    }, arg2: string, arg3: "posted" | "draft"): Promise<{
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
export function createCommerceGoodsApplication<Effect, Context = unknown>(ports: {
    clock: Clock;
    require(actor: WorkContext, permission: string): WorkContext;
    error(status: number, code: string, message: string): Error;
    hash(value: string): Promise<string>;
    forActor(actor: WorkContext): Promise<CommerceGoodsUnit<Effect>>;
    forContext(context: Context): Promise<CommerceGoodsUnit<Effect>>;
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
    async function fxAccounts(db: CommerceGoodsUnit<Effect>, actor: WorkContext, chartId: string, currency: string, gain?: string, loss?: string) {
        for (const [id, category] of [[gain, "other_income"], [loss, "other_expense"]] as const) {
            if (!id)
                throw ports.error(422, category === "other_income" ? "FX_GAIN_ACCOUNT_REQUIRED" : "FX_LOSS_ACCOUNT_REQUIRED", "Review both functional-currency FX accounts before posting a foreign goods invoice.");
            if (!await db.readFinanceAccounts1({ id: id, companyId: actor.companyId, chartId: chartId, currency: currency, category: category }))
                throw ports.error(422, "FX_ACCOUNT_INVALID", "Use active realized FX gain and loss accounts in the original Company chart and functional currency.");
        }
    }
    /** AP retains invoice-date FX; a separate source journal clears original goods carrying. */
    async function prepareCommerceInvoiceGoods(db: CommerceGoodsUnit<Effect>, actor: WorkContext, input: {
        invoice: Row;
        journal: CommerceJournal<Effect>;
        key: string;
        periodId: string;
        entryDate: string;
        gainAccountId?: string;
        lossAccountId?: string;
    }) {
        const invoice = input.invoice;
        if (invoice.source_type !== "commerce_fulfillment" || invoice.kind !== "payable")
            return null;
        const account = await db.readFinanceAccounts2({ debit_account_id: invoice.debit_account_id, companyId: actor.companyId });
        if (!account)
            throw ports.error(422, 'ACCOUNT_INVALID', 'The original goods clearing account is unavailable.');
        const chartId = account.chart_id;
        const sources = (await db.readFinanceJournalLines1({ debit_account_id: invoice.debit_account_id, debit_account_id2: invoice.debit_account_id, id: invoice.id, companyId: actor.companyId, chartId: chartId, branchId: actor.branchId, branchId2: actor.branchId, departmentId: actor.departmentId, departmentId2: actor.departmentId })).results;
        if (!sources.length)
            throw ports.error(409, "GOODS_CARRYING_SOURCE_REQUIRED", "Review the posted goods receipts in this Finance scope before posting AP.");
        const currency = input.journal.result.baseCurrency;
        if (sources.some(row => row.base_currency !== currency || row.document_currency !== invoice.currency ||
            row.stock_currency !== currency && !(row.stock_currency === null && row.document_currency === currency && row.currency === currency && Number(row.fx_rate_micros) === 1000000)))
            throw ports.error(409, "GOODS_VALUATION_REVIEW_REQUIRED", "Reconcile historical foreign goods through their owner; their original values cannot be relabelled automatically.");
        const receipts = [...new Set(sources.map(row => String(row.id)))];
        for (const id of receipts) {
            const lines = sources.filter(row => row.id === id), source = lines[0];
            if (lines.reduce((sum, row) => sum + Number(row.value_minor), 0) !== Number(source.clearing_base) ||
                lines.reduce((sum, row) => sum + Number(row.subtotal_minor), 0) !== Number(source.clearing_transaction))
                throw ports.error(409, "GOODS_CARRYING_RECONCILIATION_REQUIRED", "The original posted clearing account must exactly match received stock and document net amounts.");
        }
        const receiptBase = sources.reduce((sum, row) => sum + Number(row.value_minor), 0), invoiceBase = input.journal.preparedLines.filter(row => row.accountId === invoice.debit_account_id).reduce((sum, row) => sum + row.baseDebit - row.baseCredit, 0), difference = invoiceBase - receiptBase;
        if (invoice.currency !== currency || difference)
            await fxAccounts(db, actor, chartId, currency, input.gainAccountId, input.lossAccountId);
        const fx = difference ? await db.prepareJournal(actor, {
            chartId, periodId: input.periodId, entryDate: input.entryDate, currency, isOpening: false,
            sourceType: "commerce_invoice_goods_fx", sourceId: String(invoice.id), memo: `Original goods carrying · ${invoice.invoice_number}`,
            lines: difference > 0 ? [
                { accountId: input.lossAccountId!, debit: minorToDecimal(difference), credit: "0" },
                { accountId: String(invoice.debit_account_id), debit: "0", credit: minorToDecimal(difference) },
            ] : [
                { accountId: String(invoice.debit_account_id), debit: minorToDecimal(-difference), credit: "0" },
                { accountId: input.gainAccountId!, debit: "0", credit: minorToDecimal(-difference) },
            ],
        }, `${input.key}:goods-fx`, "posted") : null;
        const sourceJson = JSON.stringify(sources), steps: Effect[] = [];
        for (const [index, row] of sources.entries())
            steps.push(db.prepareCommerceFulfillments1({ id: row.id }), db.guardRule1({ key: input.key, guardId: `goods-carrying-${index}`, values: [row.id, invoice.id, row.version, row.journal_id, row.journal_version,
                    row.line_id, row.quantity_milli, row.value_minor, row.subtotal_minor, row.tax_minor, row.stock_currency] }));
        if (fx)
            steps.push(...fx.steps);
        for (const [accountId, category] of [[input.gainAccountId, "other_income"], [input.lossAccountId, "other_expense"]] as const)
            if (accountId)
                steps.push(db.guardRule2({ key: input.key, guardId: `goods-fx-account-${category}`, values: [accountId, actor.companyId, chartId, currency, category] }));
        const invoiceQuote = input.journal.preparedFxEvidence;
        steps.push(db.prepareCommerceInvoiceGoodsCarrying1({ id: invoice.id, companyId: actor.companyId, chartId: chartId, currency: currency, debit_account_id: invoice.debit_account_id, fxRateMicros: input.journal.result.fxRateMicros, fact6: invoiceQuote ? 'confirmed_company_quote' : 'reviewed_input', fact7: invoiceQuote?.sourceRateId ?? null, fact8: invoiceQuote?.targetRateId ?? null, fact9: invoiceQuote?.usedLatestFallback ?? false, fact10: await ports.hash(sourceJson), sourceJson: sourceJson, receiptBase: receiptBase, invoiceBase: invoiceBase, difference: difference, fact15: input.gainAccountId ?? null, fact16: input.lossAccountId ?? null, fact17: fx?.result.id ?? null, userId: actor.userId, fact19: ports.clock.now() }));
        return { steps, fxJournalId: fx?.result.id ?? null };
    }
    /** Goods returns reverse original stock separately from original invoice-date credit. */
    async function prepareReturnGoodsFx(db: CommerceGoodsUnit<Effect>, actor: WorkContext, input: {
        invoiceId: string;
        returnId: string;
        key: string;
        periodId: string;
        entryDate: string;
        stockBase: number;
        clearingAccountId: string;
        creditPostings: Array<{
            accountId: string;
            baseDebit: number;
            baseCredit: number;
        }>;
    }) {
        const clearingCredit = input.creditPostings.filter(row => row.accountId === input.clearingAccountId).reduce((sum, row) => sum + row.baseCredit - row.baseDebit, 0), difference = clearingCredit - input.stockBase;
        if (!difference)
            return null;
        const source = await db.readCommerceInvoiceGoodsCarrying1({ invoiceId: input.invoiceId, companyId: actor.companyId });
        if (!source || source.clearing_account_id !== input.clearingAccountId)
            throw ports.error(409, "RETURN_GOODS_FX_REVIEW_REQUIRED", "Review the original independently approved goods carrying and FX mappings before returning foreign stock.");
        await fxAccounts(db, actor, String(source.chart_id), String(source.stock_currency), String(source.gain_account_id ?? ""), String(source.loss_account_id ?? ""));
        // Retain the original invoice's reviewed account for every reversal, including
        // opposite partial-rounding cents; a full return reverses that account exactly.
        const amount = minorToDecimal(Math.abs(difference)), account = String(Number(source.adjustment_minor) >= 0 ? source.loss_account_id : source.gain_account_id);
        const journal = await db.prepareJournal(actor, {
            chartId: String(source.chart_id), periodId: input.periodId, entryDate: input.entryDate, currency: String(source.stock_currency),
            sourceType: "commerce_return_goods_fx", sourceId: input.returnId, memo: `Reverse original goods carrying · ${input.returnId}`, isOpening: false,
            lines: difference > 0 ? [{ accountId: input.clearingAccountId, debit: amount, credit: "0" }, { accountId: account, debit: "0", credit: amount }]
                : [{ accountId: account, debit: amount, credit: "0" }, { accountId: input.clearingAccountId, debit: "0", credit: amount }],
        }, `${input.key}:goods-fx`, "draft");
        return journal;
    }
    return { prepareCommerceInvoiceGoods, prepareReturnGoodsFx };
}
