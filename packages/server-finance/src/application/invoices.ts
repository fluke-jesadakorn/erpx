import { z } from 'zod';
import { invoiceSchema, invoicePostingSchema, type FinanceRow, type JournalLineInput, type JournalInput } from '@erpx/server-finance/domain/finance';
import type { JournalPlan } from '@erpx/server-finance/application/journal';
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { createFinancePolicy } from '@erpx/server-finance/domain/finance-policy';
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import type { FinanceError, InvoicePostingLine } from '@erpx/server-finance/domain/invoice-line-revision';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
export type InvoiceInput = z.infer<typeof invoiceSchema>;
export type InvoicePostInput = z.infer<typeof invoicePostingSchema>;
export type LineRevisionFact = {
    mode: 'native_lines' | 'whole_document';
    revision: number;
    count: number;
};
export interface InvoicePorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    now(): number;
    ready(actor: WorkContext, action: 'create' | 'post' | 'void'): Promise<void>;
    moneyScale(companyId: string, currency: string): Promise<number>;
    replay<Result=Record<string,unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result|null>;
    invoiceAccounts(actor: WorkContext, currency: string, ids: string[]): Promise<{
        id: string;
        chart_id: string;
    }[]>;
    prepareAiDraft(actor: WorkContext, proposalId: string, invoiceId: string, input: InvoiceInput, key: string): Promise<void>;
    businessBilled(actor: WorkContext, id: string): Promise<boolean>;
    editableInvoice(actor: WorkContext, id: string, version: number): Promise<{
        line_mode: string;
    } | null>;
    prepareLines(actor: WorkContext, input: InvoiceInput, id: string, revision: number, amounts: {
        subtotal: number;
        tax: number;
    }, key: string): Promise<LineRevisionFact>;
    postableInvoice(actor: WorkContext, id: string): Promise<FinanceRow | null>;
    invoiceChart(actor: WorkContext, accountId: string | number | null): Promise<{
        chart_id: string;
        base_currency: string;
    } | null>;
    prepareReviewAttachments(actor: WorkContext, id: string, version: number, setIds: string[], key: string): Promise<void>;
    nativeLines(actor: WorkContext, invoice: FinanceRow, key: string): Promise<{
        lines: InvoicePostingLine[];
    } | null>;
    matchedLines(actor: WorkContext, invoice: FinanceRow, key: string): Promise<{
        lines: JournalLineInput[];
    } | null>;
    prepareJournal(actor: WorkContext, input: JournalInput, key: string, status: 'posted'): Promise<JournalPlan>;
    prepareGoods(actor: WorkContext, input: {
        invoice: FinanceRow;
        journal: JournalPlan;
        key: string;
        periodId: string;
        entryDate: string;
        gainAccountId?: string;
        lossAccountId?: string;
    }): Promise<void>;
    preparePostedMoney(actor: WorkContext, id: string, invoice: FinanceRow, total: number): Promise<void>;
    commitDraft(facts: {
        actor: WorkContext;
        input: InvoiceInput;
        id: string;
        existingId?: string;
        invoiceId: string;
        operation: string;
        subtotal: number;
        tax: number;
        total: number;
        revision: number;
        lineRevision: LineRevisionFact;
        now: number;
    }): Promise<CommitDraftResult>;
    commitPost(facts: {
        actor: WorkContext;
        invoiceId: string;
        input: InvoicePostInput;
        id: string;
        invoice: FinanceRow;
        journal: JournalPlan;
        baseTotal: number;
        now: number;
    }): Promise<CommitPostResult>;
    commitVoid(facts: {
        actor: WorkContext;
        invoiceId: string;
        expectedVersion: number;
        key: string;
        now: number;
    }): Promise<CommitVoidResult>;
}
export function createInvoiceService(ports: InvoicePorts) {
    const { decimalToMinor, convertAtFx, requireFinanceRole } = createFinancePolicy(ports.error);
    async function createInvoice(actor: WorkContext, raw: z.input<typeof invoiceSchema>, id: string, existingId?: string) {
        actor = ports.authorize(actor, "finance.write");
        const input = invoiceSchema.parse(raw);
        await ports.ready(actor, 'create');
        const invoiceId = existingId ?? `invoice-${id}`;
        const operation = existingId ? "finance.invoice.update" : "finance.invoice.create";
        const previous = await ports.replay<CommitDraftResult>(actor, id, operation, invoiceId, input);
        if (previous)
            return previous;
        await ports.moneyScale(actor.companyId, input.currency);
        const subtotal = decimalToMinor(input.subtotal), tax = decimalToMinor(input.tax), total = subtotal + tax;
        if (subtotal <= 0 || tax < 0 || !Number.isSafeInteger(total))
            throw ports.error(422, "INVALID_INVOICE_AMOUNT", "Provide a positive subtotal and nonnegative tax within supported precision.");
        if (input.dueDate < input.invoiceDate)
            throw ports.error(422, "INVALID_DUE_DATE", "Due date cannot precede the invoice date.");
        if (tax && !input.taxAccountId)
            throw ports.error(422, "TAX_ACCOUNT_REQUIRED", "Map a separate input or output tax account before recording tax.");
        const accountIds = [input.debitAccountId, input.creditAccountId, ...(tax ? [input.taxAccountId!] : [])];
        if (new Set(accountIds).size !== accountIds.length)
            throw ports.error(422, "ACCOUNT_INVALID", "Use distinct principal and tax posting accounts.");
        const accounts = await ports.invoiceAccounts(actor, input.currency, accountIds);
        if (accounts.length !== accountIds.length || new Set(accounts.map(a => a.chart_id)).size !== 1)
            throw ports.error(422, "ACCOUNT_INVALID", "Invoice accounts must use one active chart and currency.");
        if (input.aiProposalId && !existingId)
            throw ports.error(422, "AI_DRAFT_REQUIRED", "AI coding applies only to an existing standalone draft.");
        if (input.aiProposalId)
            await ports.prepareAiDraft(actor, input.aiProposalId, invoiceId, input, id);
        if (existingId) {
            if (await ports.businessBilled(actor, existingId))
                throw ports.error(409, "BUSINESS_BILLING_OWNER_REQUIRED", "Retain the billed contract evidence. Correct this document through its billing workflow.");
            const existing = await ports.editableInvoice(actor, existingId, input.expectedVersion ?? 0);
            if (!existing)
                throw ports.error(409, "VERSION_CONFLICT", "Refresh the standalone draft before editing it.");
            if (existing.line_mode === "native_lines" && !input.lines)
                throw ports.error(422, "INVOICE_LINES_REQUIRED", "Send the complete line revision when editing a multi-line invoice.");
        }
        const revision = existingId ? (input.expectedVersion ?? 0) + 1 : 1;
        const lineRevision = await ports.prepareLines(actor, input, invoiceId, revision, { subtotal, tax }, id);
        const now = ports.now();
        return ports.commitDraft({ actor, input, id, existingId, invoiceId, operation, subtotal, tax, total, revision, lineRevision, now });
    }
    async function postInvoice(actor: WorkContext, invoiceId: string, raw: z.input<typeof invoicePostingSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        const input = invoicePostingSchema.parse(raw);
        requireFinanceRole(actor, true);
        await ports.ready(actor, 'post');
        const previous = await ports.replay<CommitPostResult>(actor, id, "finance.invoice.post", invoiceId, input);
        if (previous)
            return previous;
        const invoice = await ports.postableInvoice(actor, invoiceId);
        if (!invoice)
            throw ports.error(404, "INVOICE_NOT_FOUND", "A postable draft invoice was not found.");
        if (samePerson(actor, invoice.created_by) || !hasPermission(actor, "finance.approve"))
            throw ports.error(403, "INDEPENDENT_APPROVER_REQUIRED", "A different finance approver must post a standalone invoice.");
        if (input.expectedVersion !== undefined && input.expectedVersion !== Number(invoice.version))
            throw ports.error(409, "VERSION_CONFLICT", "The invoice changed. Refresh its Debit / Credit before posting.");
        const tax = Number(invoice.tax_minor), subtotal = Number(invoice.subtotal_minor), total = Number(invoice.total_minor);
        if (tax && !invoice.tax_account_id)
            throw ports.error(422, "TAX_ACCOUNT_REQUIRED", "This invoice needs a separate tax posting account.");
        const chart = await ports.invoiceChart(actor, invoice.debit_account_id);
        if (!chart)
            throw ports.error(422, "ACCOUNT_INVALID", "Invoice account is unavailable.");
        const payable = invoice.kind === "payable";
        const attachmentSetIds = input.attachmentSetIds ?? [];
        if (attachmentSetIds.length && !payable)
            throw ports.error(422, "SUPPLIER_BILL_REQUIRED", "Supplier bill review evidence belongs to a payable invoice.");
        if (attachmentSetIds.length)
            await ports.prepareReviewAttachments(actor, invoiceId, Number(invoice.version), attachmentSetIds, id);
        const lines: JournalLineInput[] = [{ accountId: String(invoice.debit_account_id), debit: minorToDecimal(payable ? subtotal : total), credit: "0" }, { accountId: String(invoice.credit_account_id), debit: "0", credit: minorToDecimal(payable ? total : subtotal) }];
        if (tax)
            lines.push({ accountId: String(invoice.tax_account_id), debit: payable ? minorToDecimal(tax) : "0", credit: payable ? "0" : minorToDecimal(tax) });
        const native = await ports.nativeLines(actor, invoice, id);
        const matched = await ports.matchedLines(actor, invoice, id);
        const journal = await ports.prepareJournal(actor, { chartId: chart.chart_id, periodId: input.periodId, entryDate: input.entryDate, currency: String(invoice.currency), fxRate: input.fxRate, memo: `Invoice ${invoice.invoice_number}`, sourceType: "invoice", sourceId: invoiceId, isOpening: false, lines: matched?.lines ?? native?.lines ?? lines }, id, "posted");
        await ports.prepareGoods(actor, { invoice, journal, key: id, periodId: input.periodId, entryDate: input.entryDate, gainAccountId: input.fxGainAccountId, lossAccountId: input.fxLossAccountId });
        const baseTotal = convertAtFx(total, journal.result.fxRateMicros);
        await ports.preparePostedMoney(actor, invoiceId, invoice, total);
        return ports.commitPost({ actor, invoiceId, input, id, invoice, journal, baseTotal, now: ports.now() });
    }
    async function voidDraftInvoice(actor: WorkContext, invoiceId: string, expectedVersion: number, key: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor, 'void');
        if (await ports.businessBilled(actor, invoiceId))
            throw ports.error(409, "BUSINESS_BILLING_OWNER_REQUIRED", "Correct this invoice through its retained business billing workflow.");
        return ports.commitVoid({ actor, invoiceId, expectedVersion, key, now: ports.now() });
    }
    return { createInvoice, postInvoice, voidDraftInvoice };
}


export type CommitDraftResult = { id: string; total: string; outstanding: string; status: string; version: number; };
export type CommitPostResult = { id: string; status: string; journalId: string; };
export type CommitVoidResult = { id: string; status: string; version: number; };
