import { z } from 'zod';
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { decimalToMinor, minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { thaiFinanceTaxPack } from "@erpx/feature-finance/lib/thai-finance-tax-pack";
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { TaxIdentity, TaxPermission } from '@erpx/server-finance/domain/tax-identities';
import type { FinanceError, PersistedInvoiceLine } from '@erpx/server-finance/domain/invoice-line-revision';
import type { IssueInput } from '@erpx/server-finance/domain/thai-documents';
import { withholdingAssessmentSchema, withholdingDecisionSchema, sourceInput, createWithholdingPolicy, type Row, type Source, type Assessment, type Allocation, type WorkspaceInvoice, type CapturedWithholding, type ReadyWithholding, type WithholdingInput, type WithholdingDecision, type ConsumptionInput } from '@erpx/server-finance/domain/withholding';
export interface WithholdingPorts {
    error: FinanceError;
    authorizeTax(actor: WorkContext, permission: TaxPermission): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    digest(value: unknown): string;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Record<string,unknown>|null>;
    payment(actor: WorkContext, id: string | number | null): Promise<Row | null>;
    assessment(actor: WorkContext, id: string): Promise<Assessment | null>;
    capture(actor: WorkContext, payment: Row, input: WithholdingInput): Promise<CapturedWithholding>;
    configuredOwner(actor: WorkContext, date: string): Promise<boolean>;
    configuredSupplier(actor: WorkContext, payment: Row, date: string): Promise<boolean>;
    approvedAssessment(actor: WorkContext, payment: Row): Promise<Assessment | null>;
    reviewedIdentity(actor: WorkContext, id: string, partyId: string | null, date: string): Promise<TaxIdentity>;
    consumedAssessment(actor: WorkContext, payment: Row): Promise<Row | null>;
    remitted(assessmentId: string | number | null): Promise<boolean>;
    paymentDocuments(actor: WorkContext, payment: Row): Promise<Array<{
        id: string;
    }>>;
    workspaceIdentities(actor: WorkContext): Promise<Row[]>;
    workspaceAssessments(actor: WorkContext, paymentId?: string): Promise<Row[]>;
    workspaceDocuments(actor: WorkContext, paymentId?: string): Promise<Row[]>;
    workspaceAccounts(actor: WorkContext): Promise<Row[]>;
    workspaceInvoices(actor: WorkContext, payment: Row): Promise<WorkspaceInvoice[]>;
    invoiceLines(companyId: string, id: string, revision: number): Promise<PersistedInvoiceLine[]>;
    commitPreparation(facts: {
        actor: WorkContext;
        paymentId: string;
        input: WithholdingInput;
        commandId: string;
        id: string;
        operation: string;
        payment: Row;
        captured: CapturedWithholding;
        formKind: "PND3" | "PND53";
        now: number;
    }): Promise<Record<string,unknown>>;
    commitDecision(facts: {
        actor: WorkContext;
        id: string;
        input: WithholdingDecision;
        commandId: string;
        operation: string;
        assessment: Assessment;
        current: CapturedWithholding | null;
        approvalPayment: {
            id: string | number | null;
            version: string | number | null;
        } | null;
        status: string;
        now: number;
    }): Promise<Record<string,unknown>>;
}
export function createWithholding(ports: WithholdingPorts) {
    const { safe, inPack } = createWithholdingPolicy(ports.error);
    async function prepareWithholdingAssessment(actor: WorkContext, paymentId: string, raw: z.input<typeof withholdingAssessmentSchema>, commandId: string) {
        actor = ports.authorizeTax(actor, "finance.write");
        const input = withholdingAssessmentSchema.parse(raw);
        await ports.ready(actor);
        const id = `withholding-${commandId}`, operation = "finance.withholding.prepare", previous = await ports.replay(actor, commandId, operation, id, input);
        if (previous)
            return previous;
        const payment = await ports.payment(actor, paymentId);
        if (!payment)
            throw ports.error(404, "PAYMENT_NOT_FOUND", "Select a Company supplier payment.");
        if (payment.status !== "draft" || Number(payment.version) !== input.expectedVersion || !samePerson(actor, payment.created_by))
            throw ports.error(409, "WITHHOLDING_PAYMENT_CHANGED", "The payment preparer must classify its current draft before independent approval.");
        const captured = await ports.capture(actor, payment, input), now = ports.now();
        return ports.commitPreparation({ actor, paymentId, input, commandId, id, operation, payment, captured, formKind: captured.payee.entity_kind === "resident_individual" ? "PND3" : "PND53", now });
    }
    async function decideWithholdingAssessment(actor: WorkContext, id: string, raw: z.input<typeof withholdingDecisionSchema>, commandId: string) {
        actor = ports.authorizeTax(actor, raw.action === "cancel" ? "finance.write" : "finance.approve");
        const input = withholdingDecisionSchema.parse(raw);
        await ports.ready(actor);
        const operation = `finance.withholding.${input.action}`, previous = await ports.replay(actor, commandId, operation, id, input);
        if (previous)
            return previous;
        const assessment = await ports.assessment(actor, id);
        if (!assessment)
            throw ports.error(404, "WITHHOLDING_NOT_FOUND", "The Company withholding assessment was not found.");
        if (assessment.status !== "draft" || Number(assessment.version) !== input.expectedVersion)
            throw ports.error(409, "WITHHOLDING_CHANGED", "Refresh the current pending assessment.");
        if (input.action === "cancel" ? !samePerson(actor, assessment.prepared_by) : samePerson(actor, assessment.prepared_by))
            throw ports.error(403, "INDEPENDENT_TAX_REVIEW_REQUIRED", "Only the preparer may cancel; a different Finance approver must review the tax classification.");
        const now = ports.now();
        let current: CapturedWithholding | null = null, approvalPayment: {
            id: string | number | null;
            version: string | number | null;
        } | null = null;
        if (input.action === "approve") {
            const payment = await ports.payment(actor, assessment.payment_id), source = JSON.parse(assessment.source_snapshot_json) as Source;
            if (!payment || payment.status !== "draft" || Number(payment.version) !== Number(assessment.payment_version))
                throw ports.error(409, "WITHHOLDING_PAYMENT_CHANGED", "Prepare a new assessment against the current payment draft.");
            current = await ports.capture(actor, payment, sourceInput(source, Number(payment.version), String(assessment.classification_evidence)));
            if (ports.digest(current.source) !== assessment.source_fingerprint || ports.digest(source) !== assessment.source_fingerprint)
                throw ports.error(409, "WITHHOLDING_SOURCE_CHANGED", "An invoice, reviewed legal identity or ledger changed; cancel and prepare a current assessment.");
            approvalPayment = { id: payment.id, version: payment.version };
        }
        const status = input.action === "approve" ? "approved" : input.action === "reject" ? "rejected" : "cancelled";
        return ports.commitDecision({ actor, id, input, commandId, operation, assessment, current, approvalPayment, status, now });
    }
    async function readyPaymentWithholding(actor: WorkContext, payment: Row, actualDate?: string, allocations?: Allocation[]) {
        if (payment.kind !== "payment")
            return null;
        const date = actualDate ?? String(payment.payment_date), configured = await ports.configuredSupplier(actor, payment, date);
        if (!payment.withholding_assessment_id) {
            if (configured)
                throw ports.error(422, "WITHHOLDING_REVIEW_REQUIRED", "Classify all invoice income with the Company's reviewed ordinary tax identities before payment approval.");
            return null;
        }
        const assessment = await ports.approvedAssessment(actor, payment);
        if (!assessment)
            throw ports.error(409, "WITHHOLDING_NOT_READY", "The exact independently approved assessment must remain current through bank confirmation.");
        const source = JSON.parse(assessment.source_snapshot_json) as Source, current = await ports.capture(actor, payment, sourceInput(source, Number(payment.version), String(assessment.classification_evidence)));
        if (ports.digest(source) !== assessment.source_fingerprint || ports.digest(current.source) !== assessment.source_fingerprint || Number(payment.withheld_minor) !== safe(assessment.tax_minor) || Number(payment.amount_minor) !== safe(assessment.gross_minor))
            throw ports.error(409, "WITHHOLDING_SOURCE_CHANGED", "Re-review the changed source before the payment can be released or settled.");
        if (allocations) {
            const normalized = allocations.map(item => ({ ...item, amount: minorToDecimal(decimalToMinor(item.amount)) }));
            if (ports.digest(normalized) !== ports.digest(source.allocations))
                throw ports.error(422, "WITHHOLDING_ALLOCATION_CHANGED", "Confirm the exact gross invoice allocations independently reviewed for this withholding assessment.");
        }
        if (actualDate) {
            inPack(actualDate);
            await ports.reviewedIdentity(actor, current.payer.id, null, actualDate);
            await ports.reviewedIdentity(actor, current.payee.id, String(payment.party_id), actualDate);
        }
        return { assessment, ...current };
    }
    async function assertControlledThaiTaxPaymentOwner(actor: WorkContext, paymentDate: string) {
        const configured = await ports.configuredOwner(actor, paymentDate);
        if (configured)
            throw ports.error(409, "WITHHOLDING_OWNER_REQUIRED", "This Company has reviewed Thai statutory payer evidence. Prepare and independently review the supplier payment's income classification in the Finance controlled payment workflow before its net bank result.");
    }
    function prepareWithholdingConsumption(ready: ReadyWithholding, input: ConsumptionInput): {
        documentInput: IssueInput | null;
        now: number;
    } { const { assessment, source, payer, payee } = ready, now = ports.now(); const documentInput: IssueInput | null = Number(assessment.tax_minor) === 0 ? null : { payer, payee, paymentId: input.paymentId, assessmentId: assessment.id, commandId: input.commandId, snapshot: { schema: "finance.thai.document.v1", kind: "withholding_certificate", issueDate: input.bankDate, paymentDate: input.bankDate, payer, payee, formKind: String(assessment.form_kind), incomeMinor: source.lines.filter(line => line.rateBps > 0).reduce((sum, line) => sum + line.incomeMinor, 0), taxMinor: Number(assessment.tax_minor), grossMinor: Number(assessment.gross_minor), items: source.lines.filter(line => line.rateBps > 0).map(line => ({ description: thaiFinanceTaxPack.rates[line.category].label + ": " + line.description, incomeMinor: line.incomeMinor, taxMinor: line.taxMinor, section: thaiFinanceTaxPack.rates[line.category].section, category: line.category, rateBps: line.rateBps })), source: { paymentId: input.paymentId, assessmentId: assessment.id, assessmentVersion: assessment.version, assessmentFingerprint: assessment.source_fingerprint, bankTransactionId: input.bankTransactionId, journalId: input.journalId, allocations: source.allocations, lineReceipts: source.lines, payerVersion: payer.version, payeeVersion: payee.version, rulePackVersion: thaiFinanceTaxPack.version, rulePackDigest: ports.digest(thaiFinanceTaxPack) }, edition: 1, reason: "Issued immediately with confirmed bank payment and independently reviewed ordinary withholding." } }; return { documentInput, now }; }
    async function prepareWithholdingReversal(actor: WorkContext, payment: Row) {
        if (!payment.withholding_assessment_id)
            return null;
        const assessment = await ports.consumedAssessment(actor, payment);
        if (!assessment)
            throw ports.error(409, "WITHHOLDING_REVERSAL_NOT_READY", "The payment's consumed tax receipt must be available for its financial reversal.");
        const remitted = await ports.remitted(assessment.id);
        if (remitted)
            throw ports.error(409, "WITHHOLDING_REMITTANCE_RECOVERY_REQUIRED", "Record the actual tax bank recovery before reversing a remitted supplier withholding source.");
        const docs = await ports.paymentDocuments(actor, payment), now = ports.now();
        return { assessment, docs, now };
    }
    async function withholdingWorkspace(actor: WorkContext, paymentId?: string) {
        actor = ports.authorizeTax(actor, "finance.read");
        await ports.ready(actor);
        const [identities, assessments, documents, accounts, payment] = await Promise.all([
            ports.workspaceIdentities(actor),
            ports.workspaceAssessments(actor, paymentId),
            ports.workspaceDocuments(actor, paymentId),
            ports.workspaceAccounts(actor),
            paymentId ? ports.payment(actor, paymentId) : null,
        ]);
        const invoices = payment ? await ports.workspaceInvoices(actor, payment) : [];
        const sources = await Promise.all(invoices.map(async (invoice) => {
            const current = await ports.invoiceLines(actor.companyId, invoice.id, Number(invoice.line_revision));
            const lines: Pick<PersistedInvoiceLine, "id" | "description" | "subtotal_minor" | "tax_minor">[] = current.length ? current : [{ id: `${invoice.id}:header:${invoice.line_revision}`, description: invoice.invoice_number, subtotal_minor: Number(invoice.subtotal_minor), tax_minor: Number(invoice.tax_minor) }];
            return { ...invoice, lines };
        }));
        return { identities: identities, assessments: assessments, documents: documents, accounts: accounts, payment, invoices: sources, rulePack: thaiFinanceTaxPack };
    }
    return { prepareWithholdingAssessment, decideWithholdingAssessment, readyPaymentWithholding, assertControlledThaiTaxPaymentOwner, prepareWithholdingConsumption, prepareWithholdingReversal, withholdingWorkspace };
}

