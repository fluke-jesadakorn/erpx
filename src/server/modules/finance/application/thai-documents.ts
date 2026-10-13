import { z } from 'zod';
import { samePerson } from "@/lib/identity_access/iam";
import { renderThaiDocument, type ThaiDocumentSnapshot } from "@/lib/finance/finance-thai-document";
import { thaiFinanceTaxPack } from "@/lib/finance/thai-finance-tax-pack";
import { thaiDocumentChangeSchema, thaiDocumentDecisionSchema, thaiInvoiceIssueSchema, type IssuedDocument, type IssueInput, type IssueSummary, type IssuePlan, type InvoiceIssueInput, type DocumentChangeInput, type DocumentDecision } from '../domain/thai-documents';
import type { TaxIdentity, TaxPermission } from '../domain/tax-identities';
import type { PersistedInvoiceLine } from '../domain/invoice-line-revision';
import type { FinanceRow } from '../domain/finance';
import type { FinanceError } from '../domain/invoice-line-revision';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
export interface ThaiDocumentPorts {
    error: FinanceError;
    authorizeTax(actor: WorkContext, permission: TaxPermission): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    htmlDigest(html: string): string;
    digest(value: unknown): string;
    sequence(actor: WorkContext, kind: ThaiDocumentSnapshot['kind'], year: number): Promise<number>;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Record<string,unknown>|null>;
    reviewedIdentity(actor: WorkContext, id: string, partyId: string | null, date: string): Promise<TaxIdentity>;
    invoiceLines(companyId: string, id: string, revision: number): Promise<PersistedInvoiceLine[]>;
    prepareIssue(actor: WorkContext, input: IssueInput): Promise<IssueSummary>;
    invoice(facts: {
        invoiceId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    alreadyIssued(facts: {
        invoiceId: string;
        actor: WorkContext;
    }): Promise<boolean>;
    commitTaxInvoice(facts: {
        actor: WorkContext;
        commandId: string;
        operation: string;
        input: InvoiceIssueInput;
        invoiceId: string;
        issue: IssueSummary;
    }): Promise<Record<string,unknown>>;
    document(facts: {
        id: string;
        actor: WorkContext;
    }): Promise<IssuedDocument | null>;
    events(facts: {
        id: string;
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    changes(facts: {
        id: string;
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    identities(facts: {
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    sourcePartyId(facts: {
        document: IssuedDocument;
        actor: WorkContext;
    }): Promise<string | null>;
    exportDocument(facts: {
        id: string;
        actor: WorkContext;
    }): Promise<{
        export_html: string;
        export_sha256: string;
        document_number: string;
    } | null>;
    originalForChange(facts: {
        documentId: string;
        actor: WorkContext;
    }): Promise<IssuedDocument | null>;
    sourcePartyForChange(facts: {
        original: IssuedDocument;
        actor: WorkContext;
    }): Promise<{
        party_id: string;
    } | null>;
    commitDocumentChange(facts: {
        actor: WorkContext;
        commandId: string;
        input: DocumentChangeInput;
        id: string;
        payer: TaxIdentity;
        payee: TaxIdentity;
        documentId: string;
        source: Record<string, unknown>;
        now: number;
    }): Promise<Record<string,unknown>>;
    pendingChange(facts: {
        id: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    originalForDecision(facts: {
        change: FinanceRow;
        actor: WorkContext;
    }): Promise<IssuedDocument | null>;
    financialSourceForDecision(facts: {
        original: IssuedDocument;
        actor: WorkContext;
    }): Promise<{
        party_id: string;
        status: string;
    } | null>;
    commitDocumentDecision(facts: {
        actor: WorkContext;
        commandId: string;
        operation: string;
        input: DocumentDecision;
        id: string;
        status: string;
        issue: IssueSummary | null;
        original: IssuedDocument;
        now: number;
    }): Promise<Record<string,unknown>>;
}
export function createThaiDocuments(ports: ThaiDocumentPorts) {
    async function prepareThaiDocumentIssue(actor: WorkContext, input: IssueInput): Promise<IssuePlan> {
        const { snapshot, commandId } = input, year = Number(snapshot.issueDate.slice(0, 4));
        const last = await ports.sequence(actor, snapshot.kind, year), sequence = last + 1;
        const number = `${snapshot.kind === "withholding_certificate" ? "WHT" : "VAT"}-${year}-${String(sequence).padStart(6, "0")}`, id = `thai-document-${commandId}`;
        const complete: ThaiDocumentSnapshot = { ...snapshot, documentNumber: number }, html = renderThaiDocument(complete), sha256 = ports.htmlDigest(html), fingerprint = ports.digest(complete.source), now = ports.now();
        return { id, documentNumber: number, sha256, html, complete, fingerprint, year, last, sequence, now };
    }
    async function issueThaiTaxInvoice(actor: WorkContext, invoiceId: string, raw: z.input<typeof thaiInvoiceIssueSchema>, commandId: string) {
        actor = ports.authorizeTax(actor, "finance.approve");
        const input = thaiInvoiceIssueSchema.parse(raw);
        await ports.ready(actor);
        const operation = "finance.thai-tax-invoice.issue", previous = await ports.replay(actor, commandId, operation, invoiceId, input);
        if (previous)
            return previous;
        const invoice = await ports.invoice({ invoiceId, actor });
        if (!invoice)
            throw ports.error(404, "TAX_INVOICE_SOURCE_NOT_FOUND", "A posted Company invoice is required.");
        if (!["posted", "partially_paid", "paid"].includes(String(invoice.status)) || invoice.journal_status !== "posted" || Number(invoice.version) !== input.expectedVersion)
            throw ports.error(409, "TAX_INVOICE_SOURCE_CHANGED", "Refresh the recognized invoice before issuing a VAT invoice.");
        if (invoice.kind !== "receivable" || invoice.currency !== "THB" || !(invoice.source_type === "commerce_fulfillment" || (invoice.source_type === "standalone" && invoice.line_mode === "native_lines")))
            throw ports.error(422, "TAX_INVOICE_ADAPTER_UNSUPPORTED", "The native ordinary VAT adapter requires an itemized standalone or delivered Commerce THB customer invoice; other source modules retain their owner-issued documents.");
        if (samePerson(actor, invoice.created_by))
            throw ports.error(403, "INDEPENDENT_TAX_REVIEW_REQUIRED", "A different Finance approver must issue the VAT invoice.");
        const date = String(invoice.invoice_date), payer = await ports.reviewedIdentity(actor, input.payerIdentityId, null, date), payee = await ports.reviewedIdentity(actor, input.payeeIdentityId, String(invoice.party_id), date);
        if (!payer.vat_registered)
            throw ports.error(422, "VAT_REGISTRATION_REQUIRED", "The issuer must have independently reviewed VAT registration evidence.");
        const rule = thaiFinanceTaxPack.ordinaryVat, lines = await ports.invoiceLines(actor.companyId, invoiceId, Number(invoice.line_revision));
        if (date < rule.effectiveFrom || date > rule.effectiveTo || !lines.length || lines.some(line => !["native_line", "commerce_line"].includes(line.origin_kind) || Number(line.tax_bps) !== rule.rateBps))
            throw ports.error(422, "VAT_RULES_NOT_READY", "This ordinary VAT adapter supports the source-dated 7% rate and explicit quantities; exemptions and special VAT regimes need their owner adapter.");
        if (lines.reduce((sum,line) => sum + Number(line.subtotal_minor),0) !== Number(invoice.subtotal_minor) || lines.reduce((sum,line) => sum + Number(line.tax_minor),0) !== Number(invoice.tax_minor)) throw ports.error(409,"TAX_INVOICE_SOURCE_CHANGED","Itemized source amounts no longer match the recognized invoice.");
        if (await ports.alreadyIssued({ invoiceId, actor }))
            throw ports.error(409, "TAX_INVOICE_ALREADY_ISSUED", "Review a linked document amendment instead of issuing the same source again.");
        const issue = await ports.prepareIssue(actor, { payer, payee, invoiceId, commandId, snapshot: { schema: "finance.thai.document.v1", kind: "tax_invoice", issueDate: date, payer, payee, incomeMinor: Number(invoice.subtotal_minor), taxMinor: Number(invoice.tax_minor), grossMinor: Number(invoice.total_minor), items: lines.map(line => ({ description: line.description, quantity: String(Number(line.quantity_millis) / 1000), unitPriceMinor: Number(line.unit_price_minor), incomeMinor: Number(line.subtotal_minor), taxMinor: Number(line.tax_minor) })), source: { invoiceId, invoiceVersion: invoice.version, lineRevision: invoice.line_revision, journalId: invoice.journal_id, lineSources: lines.map(line => ({ id: line.id, snapshot: JSON.parse(line.source_snapshot_json) })), rulePackVersion: thaiFinanceTaxPack.version, rulePackDigest: ports.digest(thaiFinanceTaxPack) }, edition: 1, reason: input.reason } });
        return ports.commitTaxInvoice({ actor, commandId, operation, input, invoiceId, issue });
    }
    async function thaiDocumentDetail(actor: WorkContext, id: string) {
        actor = ports.authorizeTax(actor, "finance.read");
        await ports.ready(actor);
        const document = await ports.document({ id, actor });
        if (!document)
            throw ports.error(404, "THAI_DOCUMENT_NOT_FOUND", "The issued document was not found in this Company.");
        const [events, changes, identities] = await Promise.all([
            ports.events({ id, actor }),
            ports.changes({ id, actor }),
            ports.identities({ actor }),
        ]);
        const sourcePartyId = await ports.sourcePartyId({ document, actor });
        return { document: { ...document, export_html: undefined }, snapshot: JSON.parse(document.snapshot_json) as ThaiDocumentSnapshot, sourcePartyId, events: events, changes: changes, identities: identities };
    }
    async function thaiDocumentExport(actor: WorkContext, id: string) {
        actor = ports.authorizeTax(actor, "finance.read");
        await ports.ready(actor);
        const document = await ports.exportDocument({ id, actor });
        if (!document)
            throw ports.error(404, "THAI_DOCUMENT_NOT_FOUND", "The retained issued document was not found.");
        if (ports.htmlDigest(document.export_html) !== document.export_sha256)
            throw ports.error(503, "THAI_DOCUMENT_CHECKSUM_MISMATCH", "The retained document does not match its immutable digest.");
        return { content: document.export_html, sha256: document.export_sha256, filename: `${document.document_number}.html` };
    }
    async function prepareThaiDocumentChange(actor: WorkContext, documentId: string, raw: z.input<typeof thaiDocumentChangeSchema>, commandId: string) {
        actor = ports.authorizeTax(actor, "finance.write");
        const input = thaiDocumentChangeSchema.parse(raw);
        await ports.ready(actor);
        const id = `thai-document-change-${commandId}`, previous = await ports.replay(actor, commandId, "finance.thai-document.change.prepare", id, input);
        if (previous)
            return previous;
        const original = await ports.originalForChange({ documentId, actor });
        if (!original)
            throw ports.error(404, "THAI_DOCUMENT_NOT_FOUND", "Select an issued Company document.");
        if (input.issueDate < original.issue_date)
            throw ports.error(422, "DOCUMENT_CHANGE_BEFORE_SOURCE", "An amendment cannot precede the issued source.");
        const snapshot = JSON.parse(original.snapshot_json) as ThaiDocumentSnapshot, payer = await ports.reviewedIdentity(actor, input.payerIdentityId, null, input.issueDate), party = await ports.sourcePartyForChange({ original, actor });
        if (!party)
            throw ports.error(409, "THAI_DOCUMENT_SOURCE_CHANGED", "The source legal party is unavailable.");
        const payee = await ports.reviewedIdentity(actor, input.payeeIdentityId, party.party_id, input.issueDate);
        if (original.document_kind === "tax_invoice" && !payer.vat_registered)
            throw ports.error(422, "VAT_REGISTRATION_REQUIRED", "The corrected issuer requires reviewed VAT registration.");
        const changed = { ...snapshot, payer, payee, issueDate: input.issueDate, previousDocumentNumber: original.document_number, edition: Number(original.edition) + 1, reason: input.reason }, source = { documentId, originalSha256: original.export_sha256, payerVersion: payer.version, payeeVersion: payee.version, snapshot: changed }, now = ports.now();
        return ports.commitDocumentChange({ actor, commandId, input, id, payer, payee, documentId, source, now });
    }
    async function decideThaiDocumentChange(actor: WorkContext, id: string, raw: z.input<typeof thaiDocumentDecisionSchema>, commandId: string) {
        actor = ports.authorizeTax(actor, raw.action === "cancel" ? "finance.write" : "finance.approve");
        const input = thaiDocumentDecisionSchema.parse(raw);
        await ports.ready(actor);
        const operation = `finance.thai-document.change.${input.action}`, previous = await ports.replay(actor, commandId, operation, id, input);
        if (previous)
            return previous;
        const change = await ports.pendingChange({ id, actor });
        if (!change)
            throw ports.error(404, "THAI_DOCUMENT_CHANGE_NOT_FOUND", "The document amendment was not found.");
        if (change.status !== "draft" || Number(change.version) !== input.expectedVersion)
            throw ports.error(409, "THAI_DOCUMENT_CHANGE_CHANGED", "Refresh the pending document amendment.");
        if (input.action === "cancel" ? !samePerson(actor, change.prepared_by) : samePerson(actor, change.prepared_by))
            throw ports.error(403, "INDEPENDENT_TAX_REVIEW_REQUIRED", "Only the preparer may cancel; a different Finance approver must review this amendment.");
        const original = await ports.originalForDecision({ change, actor });
        if (!original)
            throw ports.error(409, "THAI_DOCUMENT_SOURCE_CHANGED", "The immutable original document is unavailable.");
        const source = JSON.parse(String(change.source_snapshot_json)) as {
            originalSha256: string;
            payerVersion: number;
            payeeVersion: number;
            snapshot: ThaiDocumentSnapshot;
        }, now = ports.now();
        let issue: IssueSummary | null = null;
        if (input.action === "approve") {
            const party = await ports.financialSourceForDecision({ original, actor });
            if (!party || !(original.payment_id ? ["settled"] : ["posted", "partially_paid", "paid"]).includes(party.status) || source.originalSha256 !== original.export_sha256 || ports.digest(source) !== change.source_fingerprint)
                throw ports.error(409, "THAI_DOCUMENT_SOURCE_CHANGED", "The retained document or settled source changed before amendment review.");
            const payer = await ports.reviewedIdentity(actor, String(change.corrected_payer_identity_id), null, String(change.issue_date)), payee = await ports.reviewedIdentity(actor, String(change.corrected_payee_identity_id), party.party_id, String(change.issue_date));
            if (Number(payer.version) !== Number(source.payerVersion) || Number(payee.version) !== Number(source.payeeVersion))
                throw ports.error(409, "THAI_DOCUMENT_IDENTITY_CHANGED", "Review a new amendment against the current approved identities.");
            const snapshot = { ...source.snapshot, payer, payee, source: { ...source.snapshot.source, previousDocumentId: original.id, previousSha256: original.export_sha256, amendmentId: id, payerVersion: payer.version, payeeVersion: payee.version } };
            issue = await ports.prepareIssue(actor, { payer, payee, snapshot, previous: original, paymentId: original.payment_id ?? undefined, invoiceId: original.invoice_id ?? undefined, assessmentId: original.assessment_id ?? undefined, commandId });
        }
        const status = input.action === "approve" ? "issued" : input.action === "reject" ? "rejected" : "cancelled";
        return ports.commitDocumentDecision({ actor, commandId, operation, input, id, status, issue, original, now });
    }
    return { prepareThaiDocumentIssue, issueThaiTaxInvoice, thaiDocumentDetail, thaiDocumentExport, prepareThaiDocumentChange, decideThaiDocumentChange };
}

