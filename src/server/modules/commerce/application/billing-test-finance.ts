import type { WorkContext } from "../../identity_access/public/authorization";
import type { Clock } from "../../shared/application/ports";
import type { AttachmentManifestItem } from "../../files/public/attachments/types";
import type { FunctionalFxEvidence } from "../../finance/public/commerce-journal-contracts";
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
import type { StripeObject } from "../../billing/public/billing-stripe";
export type BillingTestFinanceUnit<Effect> = {
    readBillingTestFinanceDocuments1(facts: {
        stripe_id: string;
    }): Promise<{
        source_billing_invoice_id: string;
        customer_holding_id: string;
        total_satang: number;
    } | null>;
    writeBillingTestFinanceDocuments1(facts: {
        stripe_id: string;
        id: string;
        holding_id: string;
        TEST_COMPANY: "erpx_billing_sandbox";
        total_satang: number;
        status: string;
        now: number;
        now2: number;
    }): Promise<CommerceRows<Record<string, unknown>>>;
    writeBillingTestFinanceDocuments2(facts: {
        status: string;
        now: number;
        stripe_id: string;
    }): Promise<CommerceRows<Record<string, unknown>>>;
    readBillingTestFinanceEntries1(facts: {
        key: string;
    }): Promise<{
        entry_kind: string;
        amount_satang: number;
    } | null>;
    writeBillingTestFinanceEntries1(facts: {
        key: string;
        stripe_id: string;
        kind: "void" | "issued" | "receipt" | "credit" | "credit_reversal";
        amount: number;
        now: number;
        now2: number;
    }): Promise<CommerceRows<Record<string, unknown>>>;
    writeBillingTestFinanceLines1(facts: {
        key: string;
        fact1: 0;
        debit: Account;
        amount: number;
        key2: string;
        fact5: 1;
        credit: Account;
        amount2: number;
    }): Promise<CommerceRows<Record<string, unknown>>>;
    readBillingTestFinanceEntries2(facts: {
        issuedKey: string;
    }): Promise<Record<string, unknown> | null>;
    readBillingTestFinanceEntries3(facts: {
        fact0: string;
    }): Promise<Record<string, unknown> | null>;
    readBillingTestFinanceEntries4(facts: {
        fact0: string;
    }): Promise<Record<string, unknown> | null>;
    readBillingTestFinanceEntries5(facts: {
        stripe_id: string;
    }): Promise<{
        issued: number;
        received: number;
        reversed: number;
        credited: number;
    } | null>;
    readBillingTestFinanceLines1(facts: {
        stripe_id: string;
    }): Promise<{
        balance: number;
    } | null>;
    transaction<Result>(work: (unit: BillingTestFinanceUnit<Effect>) => Promise<Result>): Promise<Result>;
};
export type Account = "accounts_receivable" | "revenue" | "sandbox_cash" | "customer_credit";
export type LocalInvoice = {
    id: string;
    holding_id: string;
    stripe_id: string;
    total_satang: number;
};
export type Credit = {
    id: string;
    pre: number;
    post: number;
    status: "issued" | "void";
};
export function createBillingTestFinanceApplication<Effect, Context = unknown>(ports: {
    clock: Clock;
    require(actor: WorkContext, permission: string): WorkContext;
    error(status: number, code: string, message: string): Error;
    hash(value: string): Promise<string>;
    forActor(actor: WorkContext): Promise<BillingTestFinanceUnit<Effect>>;
    forContext(context: Context): Promise<BillingTestFinanceUnit<Effect>>;
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
    const TEST_COMPANY = "erpx_billing_sandbox";
    function satang(value: unknown, field: string): number {
        if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0)
            throw new Error(`INVALID_STRIPE_${field}`);
        return value;
    }
    /** A test Company ledger physically owned by ERPX Holding h003, separate from Finance reporting. */
    async function syncBillingTestFinance(env: Context, local: LocalInvoice, stripeInvoice: StripeObject, creditNotes: StripeObject[], now = ports.clock.now()) {
        if (!local.stripe_id || stripeInvoice.id !== local.stripe_id || stripeInvoice.livemode !== false)
            throw new Error("TEST_FINANCE_STRIPE_SCOPE_MISMATCH");
        if (stripeInvoice.currency !== "thb" || satang(stripeInvoice.total, "TOTAL") !== Number(local.total_satang))
            throw new Error("TEST_FINANCE_AMOUNT_MISMATCH");
        const metadata = stripeInvoice.metadata as Record<string, unknown> | undefined;
        if (metadata?.erpx_invoice_id !== local.id || metadata?.holding_id !== local.holding_id)
            throw new Error("TEST_FINANCE_SOURCE_MISMATCH");
        const status = String(stripeInvoice.status);
        if (!["draft", "open", "paid", "void", "uncollectible"].includes(status))
            throw new Error("TEST_FINANCE_STATUS_INVALID");
        const paid = satang(stripeInvoice.amount_paid ?? 0, "AMOUNT_PAID");
        if (status !== "paid" && paid > 0)
            throw new Error("PARTIAL_PAYMENT_REQUIRES_RECONCILIATION");
        if (status === "paid" && paid === 0 && Number(local.total_satang) > 0)
            throw new Error("PAID_AMOUNT_MISSING");
        const credits: Credit[] = creditNotes.map(note => {
            if (note.livemode !== false || note.invoice !== local.stripe_id ||
                !["issued", "void"].includes(String(note.status)))
                throw new Error("TEST_FINANCE_CREDIT_SCOPE_MISMATCH");
            return { id: note.id, pre: satang(note.pre_payment_amount ?? 0, "PRE_CREDIT"),
                post: satang(note.post_payment_amount ?? 0, "POST_CREDIT"), status: note.status as Credit["status"] };
        });
        const unique = new Set(credits.map(note => note.id));
        if (unique.size !== credits.length)
            throw new Error("TEST_FINANCE_DUPLICATE_CREDIT");
        const totalCredits = credits.reduce((sum, note) => note.status === "issued" ? sum + note.pre + note.post : sum, 0);
        if (totalCredits > Number(local.total_satang))
            throw new Error("TEST_FINANCE_CREDIT_EXCEEDS_INVOICE");
        const activePreCredits = credits.reduce((sum, note) => note.status === "issued" ? sum + note.pre : sum, 0);
        if (status === "paid" && paid + activePreCredits !== Number(local.total_satang))
            throw new Error("TEST_FINANCE_PAYMENT_RECONCILIATION_REQUIRED");
        const write = await ports.forContext(env);
        return write.transaction(async (tx) => {
            const existing = await tx.readBillingTestFinanceDocuments1({ stripe_id: local.stripe_id });
            if (existing && (existing.source_billing_invoice_id !== local.id ||
                existing.customer_holding_id !== local.holding_id || Number(existing.total_satang) !== Number(local.total_satang)))
                throw new Error("TEST_FINANCE_DOCUMENT_CONFLICT");
            if (!existing)
                await tx.writeBillingTestFinanceDocuments1({ stripe_id: local.stripe_id, id: local.id, holding_id: local.holding_id, TEST_COMPANY: TEST_COMPANY, total_satang: local.total_satang, status: status, now: now, now2: now });
            else
                await tx.writeBillingTestFinanceDocuments2({ status: status, now: now, stripe_id: local.stripe_id });
            async function entry(key: string, kind: "issued" | "receipt" | "void" | "credit" | "credit_reversal", amount: number, debit: Account, credit: Account) {
                if (amount === 0)
                    return;
                const prior = await tx.readBillingTestFinanceEntries1({ key: key });
                if (prior) {
                    if (prior.entry_kind !== kind || Number(prior.amount_satang) !== amount)
                        throw new Error("TEST_FINANCE_ENTRY_CONFLICT");
                    return;
                }
                await tx.writeBillingTestFinanceEntries1({ key: key, stripe_id: local.stripe_id, kind: kind, amount: amount, now: now, now2: now });
                await tx.writeBillingTestFinanceLines1({ key: key, fact1: 0, debit: debit, amount: amount, key2: key, fact5: 1, credit: credit, amount2: amount });
            }
            const issuedKey = `${local.stripe_id}:issued`;
            const priorIssue = await tx.readBillingTestFinanceEntries2({ issuedKey: issuedKey });
            const issued = status !== "draft" && (status !== "void" || !!priorIssue);
            if (issued)
                await entry(issuedKey, "issued", Number(local.total_satang), "accounts_receivable", "revenue");
            if (issued)
                for (const note of credits) {
                    if (note.status === "issued") {
                        await entry(`${note.id}:pre`, "credit", note.pre, "revenue", "accounts_receivable");
                        await entry(`${note.id}:post`, "credit", note.post, "revenue", "customer_credit");
                    }
                    else {
                        const priorPre = await tx.readBillingTestFinanceEntries3({ fact0: `${note.id}:pre` });
                        const priorPost = await tx.readBillingTestFinanceEntries4({ fact0: `${note.id}:post` });
                        if (priorPre)
                            await entry(`${note.id}:pre:reversal`, "credit_reversal", note.pre, "accounts_receivable", "revenue");
                        if (priorPost)
                            await entry(`${note.id}:post:reversal`, "credit_reversal", note.post, "customer_credit", "revenue");
                    }
                }
            if (status === "paid")
                await entry(`${local.stripe_id}:receipt`, "receipt", paid, "sandbox_cash", "accounts_receivable");
            if (status === "void" && issued) {
                const remaining = Number(local.total_satang) - activePreCredits;
                if (remaining < 0)
                    throw new Error("TEST_FINANCE_VOID_EXCEEDS_RECEIVABLE");
                await entry(`${local.stripe_id}:void`, "void", remaining, "revenue", "accounts_receivable");
            }
            const result = await tx.readBillingTestFinanceEntries5({ stripe_id: local.stripe_id });
            if (!result)
                throw new Error("TEST_FINANCE_RESULT_MISSING");
            const ar = await tx.readBillingTestFinanceLines1({ stripe_id: local.stripe_id });
            const receivable = Number(ar?.balance ?? 0);
            if (receivable < 0)
                throw new Error("TEST_FINANCE_NEGATIVE_RECEIVABLE");
            return { companyId: TEST_COMPANY, invoiceSatang: Number(local.total_satang),
                receivableSatang: receivable, receivedSatang: Number(result.received),
                reversedSatang: Number(result.reversed), creditedSatang: Number(result.credited),
                stripeStatus: status };
        });
    }
    return { syncBillingTestFinance };
}
