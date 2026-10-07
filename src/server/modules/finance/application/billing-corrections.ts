import type {RecognitionItem} from "../domain/finance-contract-policy";
import { z } from 'zod';
import { samePerson } from '@/lib/iam';
import { decimalToMinor, minorToDecimal } from '@/lib/finance-money';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
import type { JournalInput } from '../domain/finance';
import type { ReversalInput } from '../domain/company-money';
import { businessContractSchema, businessBillingSchema, contractBillingCalculation, type BusinessContractInput } from '../domain/finance-contract-policy';
import { billingCorrectionSchema, billingCorrectionDecisionSchema, type Input, type Row, type CorrectionDecision, type CorrectionSource, type CorrectionConfiguration, type CorrectionResponse, type ReplacementInput, type CreditSummary } from '../domain/billing-corrections';
export interface BillingCorrectionPorts {
    error: FinanceError;
    permission(actor: WorkContext, permission: 'finance.write' | 'finance.approve'): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    transaction<Result>(actor: WorkContext, work: () => Promise<Result>): Promise<Result>;
    now(): number;
    id(): string;
    hash(value: string): Promise<string>;
    replay<Result=Record<string,unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result|null>;
    requireTool(actor: WorkContext): Promise<void>;
    configuration(actor: WorkContext, input: BusinessContractInput): Promise<CorrectionConfiguration>;
    bill(facts: {
        billId: string;
        actor: WorkContext;
    }): Promise<Row | null>;
    hasChild(facts: {
        bill: Row;
        actor: WorkContext;
    }): Promise<unknown | null>;
    contract(facts: {
        bill: Row;
        actor: WorkContext;
    }): Promise<Row | null>;
    invoice(facts: {
        bill: Row;
        actor: WorkContext;
    }): Promise<Row | null>;
    openPeriod(facts: {
        input: Input;
        actor: WorkContext;
    }): Promise<unknown | null>;
    externalCredit(facts: {
        invoice: Row;
        actor: WorkContext;
    }): Promise<unknown | null>;
    recognitionRuns(facts: {
        bill: Row;
        actor: WorkContext;
    }): Promise<Row[]>;
    journalLines(facts: {
        actor: WorkContext;
        invoice: Row;
        bill: Row;
    }): Promise<Row[]>;
    allocations(facts: {
        invoice: Row;
        actor: WorkContext;
    }): Promise<Row[]>;
    existing(actor: WorkContext, billId: string): Promise<boolean>;
    correction(actor: WorkContext, id: string): Promise<Row | null>;
    prepareDecision(actor: WorkContext, id: string, key: string, expectedVersion: number): Promise<void>;
    commitPrepare(facts: {
        actor: WorkContext;
        billId: string;
        input: Input;
        key: string;
        id: string;
        op: string;
        request: Input & {
            billId: string;
        };
        s: CorrectionSource;
        now: number;
    }): Promise<CommitPrepareResult>;
    commitReject(facts: {
        actor: WorkContext;
        id: string;
        key: string;
        op: string;
        input: CorrectionDecision;
        version: number;
        now: number;
    }): Promise<CommitRejectResult>;
    prepareCredit(actor: WorkContext, input: JournalInput, key: string): Promise<CreditSummary>;
    reverseRecognition(facts: {
        actor: WorkContext;
        run: Row;
        request: Input;
        key: string;
        reason: string;
        now: number;
    }): Promise<void>;
    prepareMoneyReversal(actor: WorkContext, input: ReversalInput): Promise<void>;
    commitPosted(facts: {
        actor: WorkContext;
        key: string;
        id: string;
        op: string;
        input: CorrectionDecision;
        version: number;
        now: number;
        s: CorrectionSource;
        row: Row;
        credit: CreditSummary;
        credited: number;
        refund: number;
        refundId: string | null;
        replacementBillId: string;
        decisionId: string;
        decisionHash: string;
        response: CorrectionResponse;
    }): Promise<void>;
    prepareReplacement(facts: {
        actor: WorkContext;
        contractId: string;
        input: z.infer<typeof businessBillingSchema>;
        key: string;
        revision: ReplacementInput;
        correctionId: string;
    }): Promise<{
        invoiceId?: string | null;
    }>;
    reassignDraftCreator(actor: WorkContext, invoiceId: string, createdBy: Row[string]): Promise<void>;
    postReplacement(actor: WorkContext, id: string, input: {
        periodId: string;
        entryDate: string;
        expectedVersion: number;
    }, key: string): Promise<void>;
    approveNoCharge(actor: WorkContext, id: string, reason: string): Promise<void>;
}
export function createBillingCorrections(ports: BillingCorrectionPorts) {
    function companyActor(actor: WorkContext, permission: 'finance.write' | 'finance.approve') {
        actor = ports.permission(actor, permission);
        if (actor.branchId || actor.departmentId)
            throw ports.error(403, 'COMPANY_FINANCE_REQUIRED', 'A posted billing correction requires Company Finance authority.');
        return actor;
    }
    async function source(actor: WorkContext, billId: string, input: Input) {
        const bill = await ports.bill({ billId, actor });
        if (!bill)
            throw ports.error(404, 'BILLING_RUN_NOT_FOUND', 'Billing run not found in this Company.');
        if (bill.version !== input.expectedVersion || !bill.invoice_id || await ports.hasChild({ bill, actor }))
            throw ports.error(409, 'BILLING_REVISION_SOURCE_CHANGED', 'Correct the current posted billing edition.');
        const contract = await ports.contract({ bill, actor });
        if (!contract || !['active', 'cancelled', 'superseded'].includes(String(contract.status)))
            throw ports.error(409, 'CONTRACT_CHANGED', 'Retained approved contract evidence is required.');
        const content = businessContractSchema.parse({ ...JSON.parse(String(bill.contract_content_json ?? contract.content_json)), ...(input.pricing ? { pricing: input.pricing } : {}), ...(input.obligations ? { obligations: input.obligations } : {}) });
        const config = await ports.configuration(actor, content);
        const invoice = await ports.invoice({ bill, actor });
        const oldResult = JSON.parse(String(bill.result_json));
        if (!invoice || !['posted', 'partially_paid', 'paid'].includes(String(invoice.status)) || invoice.journal_status !== 'posted' || invoice.version !== input.invoiceExpectedVersion || invoice.kind !== 'receivable' || invoice.currency !== content.currency || invoice.chart_id !== content.chartId || invoice.debit_account_id !== content.receivableAccountId || invoice.credit_account_id !== content.deferredAccountId || Number(invoice.line_revision) !== 1 || Number(invoice.subtotal_minor) !== decimalToMinor(oldResult.subtotal) || Number(invoice.tax_minor) !== decimalToMinor(oldResult.tax) || Number(invoice.base_total_minor) !== Number(invoice.total_minor) || Number(invoice.base_outstanding_minor) !== Number(invoice.outstanding_minor) || Number(invoice.fx_rate_micros) !== 1000000)
            throw ports.error(409, 'BILLING_POSTED_SOURCE_CHANGED', 'Refresh the original posted functional-currency invoice and its reconciled control balance.');
        if (input.entryDate < String(invoice.posted_date) || input.billing.invoiceDate !== input.entryDate || input.billing.dueDate < input.entryDate)
            throw ports.error(422, 'BILLING_CORRECTION_DATE_INVALID', 'Date the credit and replacement together, on or after the original accounting date.');
        if (!await ports.openPeriod({ input, actor }))
            throw ports.error(409, 'PERIOD_NOT_OPEN', 'Choose an open period for the correction date.');
        if (await ports.externalCredit({ invoice, actor }))
            throw ports.error(409, 'BILLING_EXTERNAL_CREDIT_EXISTS', 'Resolve other source credits before correcting business billing.');
        const runs = await ports.recognitionRuns({ bill, actor });
        if (runs.some(r => r.status === 'draft' || r.status === 'posted' && String(r.entry_date) > input.entryDate))
            throw ports.error(409, 'BILLING_RECOGNITION_REVIEW_REQUIRED', 'Resolve draft recognition and date the correction after all posted recognition.');
        const journals = await ports.journalLines({ actor, invoice, bill });
        const allocations = await ports.allocations({ invoice, actor });
        let result: ReturnType<typeof contractBillingCalculation>;
        try {
            result = contractBillingCalculation(content, businessBillingSchema.parse({ ...input.billing, expectedVersion: contract.version }));
        }
        catch (e) {
            throw ports.error(422, 'BILLING_INPUT_INVALID', e instanceof Error ? e.message : 'Invalid corrected billing.');
        }
        if (result.billingKey !== bill.billing_key || result.serviceFrom !== bill.service_from || result.serviceTo !== bill.service_to)
            throw ports.error(422, 'BILLING_REVISION_WINDOW_CHANGED', 'Retain the exact original service window.');
        const evidence = { bill, contract, invoice, runs, journals, allocations, config, content, result };
        return { ...evidence, sourceHash: await ports.hash(JSON.stringify(evidence)) };
    }
    async function prepareBillingCorrection(actor: WorkContext, billId: string, raw: z.input<typeof billingCorrectionSchema>, key: string) { actor = companyActor(actor, 'finance.write'); const input = billingCorrectionSchema.parse(raw), id = `billing-correction-${key}`, op = 'finance.billing-correction.prepare', request = { billId, ...input }; await ports.ready(actor); return ports.transaction(actor, async () => { const prior = await ports.replay<CommitPrepareResult>(actor, key, op, id, request); if (prior)
        return prior; await ports.requireTool(actor); const s = await source(actor, billId, input); if (await ports.existing(actor, billId))
        throw ports.error(409, 'BILLING_CORRECTION_EXISTS', 'Review the retained correction for this bill.'); return ports.commitPrepare({ actor, billId, input, key, id, op, request, s, now: ports.now() }); }); }
    async function decideBillingCorrection(actor: WorkContext, id: string, raw: z.input<typeof billingCorrectionDecisionSchema>, action: 'post' | 'reject', key: string) { actor = companyActor(actor, 'finance.approve'); const input = billingCorrectionDecisionSchema.parse(raw), op = `finance.billing-correction.${action}`; await ports.ready(actor); return ports.transaction(actor, async () => { const prior = await ports.replay<CommitRejectResult|PostedBillingCorrectionResult>(actor, key, op, id, input); if (prior)
        return prior; const row = await ports.correction(actor, id); if (!row)
        throw ports.error(404, 'BILLING_CORRECTION_NOT_FOUND', 'Billing correction not found in this Company.'); if (row.status !== 'draft' || row.version !== input.expectedVersion)
        throw ports.error(409, 'BILLING_CORRECTION_CHANGED', 'Refresh the prepared correction before deciding.'); if (samePerson(actor, row.prepared_by))
        throw ports.error(403, 'INDEPENDENT_APPROVER_REQUIRED', 'Another Finance person must review the credit, allocation and refund evidence.'); await ports.prepareDecision(actor, id, key, input.expectedVersion); const now = ports.now(), version = input.expectedVersion + 1; if (action === 'reject')
        return ports.commitReject({ actor, id, key, op, input, version, now }); await ports.requireTool(actor); const request = billingCorrectionSchema.parse(JSON.parse(String(row.input_json))), s = await source(actor, String(row.billing_run_id), request); if (s.sourceHash !== row.source_hash)
        throw ports.error(409, 'BILLING_CORRECTION_SOURCE_CHANGED', 'Payments, recognition, terms or account evidence changed. Reject this correction and prepare current evidence.'); const credit = await ports.prepareCredit(actor, { chartId: s.content.chartId, periodId: request.periodId, entryDate: request.entryDate, currency: s.content.currency, memo: `Credit ${row.number} for retained invoice ${s.invoice.invoice_number}`, sourceType: 'business_billing_credit', sourceId: id, isOpening: false, lines: s.journals.filter(l => l.id === s.invoice.journal_id).map(l => ({ accountId: String(l.account_id), description: String(l.description), debit: minorToDecimal(Number(l.credit_minor)), credit: minorToDecimal(Number(l.debit_minor)), dimensions: JSON.parse(String(l.dimension_json)) })) }, `${key}:credit`); for (const run of s.runs.filter(r => r.status === 'posted'))
        await ports.reverseRecognition({ actor, run, request, key: `${key}:recognition:${run.id}`, reason: input.reason, now }); const credited = Number(s.invoice.outstanding_minor), refund = Number(s.invoice.total_minor) - credited, refundId = refund ? `refund-${id}` : null, replacementKey = `${key}:replacement`, replacementBillId = `business-bill-${replacementKey}`; await ports.prepareMoneyReversal(actor, { sourceType: 'finance_billing_correction', sourceId: id, amountKind: 'credit_total', transactionDate: request.entryDate, amountUnits: String(s.invoice.total_minor), original: { sourceType: 'finance_invoice', sourceId: String(s.invoice.id), amountKind: 'total' } }); const response = { id, status: 'posted', version, creditJournalId: credit.result.id, replacementBillId, creditedMinor: credited, refundMinor: refund, refundId }; await ports.commitPosted({ actor, key, id, op, input, version, now, s, row, credit, credited, refund, refundId, replacementBillId, decisionId: ports.id(), decisionHash: await ports.hash(JSON.stringify(input)), response }); const replacement = await ports.prepareReplacement({ actor, contractId: String(s.bill.contract_id), input: { ...request.billing, expectedVersion: Number(s.contract.version) }, key: replacementKey, revision: { expectedVersion: request.expectedVersion, invoiceExpectedVersion: Number(s.invoice.version) + 1, billing: request.billing, pricing: request.pricing, obligations: request.obligations, reason: request.reason, parentId: String(s.bill.id) }, correctionId: id }); if (replacement.invoiceId) {
        await ports.reassignDraftCreator(actor, String(replacement.invoiceId), row.prepared_by);
        await ports.postReplacement(actor, String(replacement.invoiceId), { periodId: request.periodId, entryDate: request.entryDate, expectedVersion: 1 }, `${key}:replacement-post`);
    }
    else
        await ports.approveNoCharge(actor, replacementBillId, input.reason); return response; }); }
    return { prepareBillingCorrection, decideBillingCorrection, source };
}


export type CommitPrepareResult = { id: string; status: string; version: number; result: { serviceFrom: string; serviceTo: string; billingKey: string; subtotal: string; tax: string; total: string; allocations: { key: string; amount: string; }[]; items: RecognitionItem[]; }; creditTotal: string; sourceHash: string; };
export type CommitRejectResult = { id: string; status: string; version: number; };

export type PostedBillingCorrectionResult={id:string;status:string;version:number;creditJournalId:string;replacementBillId:string;creditedMinor:number;refundMinor:number;refundId:string|null};
