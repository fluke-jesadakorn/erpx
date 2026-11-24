import { idCompanyStatusVersionGuard } from './finance-read-tail-r20-sql';
import { INVOICE_EXISTS_PREFIX } from './finance-read-tail-r30-sql';
import { ISSUED_THAI_DOCUMENT_BY_ID_SQL } from './finance-read-tail-r31-sql';
import {withFinancePurchasedCommand,withFinancePurchasedRead} from "@erpx/server-finance/adapters/purchase-boundary";
import { FINANCE_ADVISORY_XACT_LOCK_SQL } from "./advisory-lock-dedupe-sql";
import { nativeFinanceThaiDocuments } from '@erpx/server-collaboration/adapters/native-finance-thai-documents';
import { salesInvoiceLines } from "@erpx/server-finance/adapters/sales-lines";
import type { IssueSummary } from '@erpx/server-finance/domain/thai-documents';
import { createThaiDocuments } from '@erpx/server-finance/application/thai-documents';
import type { FinanceRow } from '@erpx/server-finance/domain/finance';
import "server-only";
import { createHash } from "node:crypto";
import { z } from "zod";
import { type ThaiDocumentSnapshot } from "@erpx/feature-finance/lib/finance-thai-document";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { currentTaxSignerGuards, requireCompanyTaxAuthority, reviewedTaxIdentity, taxIdentityGuards, taxSourceDigest, type TaxIdentity } from "@erpx/server-finance/public/finance-tax-identities";
export { thaiDocumentChangeSchema, thaiDocumentDecisionSchema, thaiInvoiceIssueSchema } from '@erpx/server-finance/domain/thai-documents';
import { thaiDocumentChangeSchema, thaiDocumentDecisionSchema, thaiInvoiceIssueSchema, type IssuedDocument } from '@erpx/server-finance/domain/thai-documents';
export async function prepareThaiDocumentIssue(db: SqlDatabase, actor: WorkContext, input: {
    snapshot: Omit<ThaiDocumentSnapshot, "documentNumber">;
    payer: TaxIdentity;
    payee: TaxIdentity;
    paymentId?: string;
    invoiceId?: string;
    assessmentId?: string;
    previous?: IssuedDocument;
    commandId: string;
}): Promise<IssueSummary & {
    steps: SqlStatement[];
}> {
    const { snapshot, payer, payee, commandId } = input;
    const { id, documentNumber: number, sha256, html, complete, fingerprint, year, last, now } = await documentService(db).prepareThaiDocumentIssue(actor, input);
    return { id, documentNumber: number, sha256, html, steps: [
            ...(snapshot.kind === "withholding_certificate" ? [db.prepare(FINANCE_ADVISORY_XACT_LOCK_SQL).bind(`finance-withholding-month:${actor.companyId}:${String(snapshot.paymentDate).slice(0, 7)}:${snapshot.formKind}`)] : []),
            ...(snapshot.kind === "withholding_certificate" ? await currentTaxSignerGuards(db, actor, payer, commandId) : []),
            ...taxIdentityGuards(db, actor, commandId, [payer, payee]),
            await nativeFinanceThaiDocuments(db, actor.companyId).sequenceSeed([actor.companyId, snapshot.kind, year]),
            assertion(db, commandId, "thai-document-number", "EXISTS(SELECT 1 FROM finance_thai_document_sequences WHERE company_id=? AND document_kind=? AND fiscal_year=? AND last_number=?)", [actor.companyId, snapshot.kind, year, last]),
            await nativeFinanceThaiDocuments(db, actor.companyId).sequenceBump([actor.companyId, snapshot.kind, year, last]),
            await nativeFinanceThaiDocuments(db, actor.companyId).issue([id, actor.companyId, snapshot.kind, number, snapshot.issueDate, input.paymentId ?? null, input.invoiceId ?? null, input.assessmentId ?? null, input.previous?.id ?? null, snapshot.edition, payer.id, payee.id, fingerprint, JSON.stringify(complete), html, sha256, snapshot.reason, actor.userId, commandId, now]),
            await nativeFinanceThaiDocuments(db, actor.companyId).eventIssued([`thai-event-${commandId}`, id, actor.companyId, snapshot.reason, actor.userId, commandId, now]),
        ] };
}
export async function issueThaiTaxInvoice(actor: WorkContext, invoiceId: string, input: z.input<typeof thaiInvoiceIssueSchema>, commandId: string) { return withFinancePurchasedCommand(actor,{key:commandId,operation:"finance.thai-tax-invoice.issue",purpose:"settle-existing",capability:"finance.tax"},tx=>documentService(tx).issueThaiTaxInvoice(actor, invoiceId, input, commandId)); }
export async function thaiDocumentDetail(actor: WorkContext, id: string) { return withFinancePurchasedRead(actor,db=>documentService(db).thaiDocumentDetail(actor, id),"finance.tax"); }
export async function thaiDocumentExport(actor: WorkContext, id: string) { return withFinancePurchasedRead(actor,db=>documentService(db).thaiDocumentExport(actor, id),"finance.tax"); }
export async function prepareThaiDocumentChange(actor: WorkContext, documentId: string, input: z.input<typeof thaiDocumentChangeSchema>, commandId: string) { return withFinancePurchasedCommand(actor,{key:commandId,operation:"finance.thai-document.change.prepare",purpose:"settle-existing",capability:"finance.tax"},tx=>documentService(tx).prepareThaiDocumentChange(actor, documentId, input, commandId)); }
export async function decideThaiDocumentChange(actor: WorkContext, id: string, input: z.input<typeof thaiDocumentDecisionSchema>, commandId: string) { return withFinancePurchasedCommand(actor,{key:commandId,operation:`finance.thai-document.change.${input.action}`,purpose:"settle-existing",capability:"finance.tax"},tx=>documentService(tx).decideThaiDocumentChange(actor, id, input, commandId)); }
function documentService(provided?: SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined = provided ? Promise.resolve(provided) : undefined;
    const db = (actor: WorkContext) => pending ??= holdingResources(actor.holdingId).then(r => r.write);
    const issues = new Map<string, Awaited<ReturnType<typeof prepareThaiDocumentIssue>>>();
    return createThaiDocuments({ error: (...args) => new ApiError(...args), authorizeTax: requireCompanyTaxAuthority, ready: async (actor) => { await db(actor); }, now: Date.now, htmlDigest: html => createHash('sha256').update(html, 'utf8').digest('hex'), digest: taxSourceDigest, sequence: async (actor, kind, year) => Number(await (await db(actor)).prepare('SELECT last_number FROM finance_thai_document_sequences WHERE company_id=? AND document_kind=? AND fiscal_year=?').bind(actor.companyId, kind, year).first('last_number') ?? 0), replay: async (actor, key, operation, id, input) => replayCommand(await db(actor), actor, key, operation, id, input), reviewedIdentity: async (actor, id, partyId, date) => reviewedTaxIdentity(await db(actor), actor, id, partyId, date), invoiceLines: async (companyId, id, revision) => salesInvoiceLines(await pending!, companyId, id, revision), prepareIssue: async (actor, input) => { const issue = await prepareThaiDocumentIssue(await db(actor), actor, input); issues.set(issue.id, issue); return { id: issue.id, documentNumber: issue.documentNumber, sha256: issue.sha256, html: issue.html }; }, invoice: async (facts) => { const { invoiceId, actor } = facts; const write = await db(actor); return write.prepare("SELECT i.*,j.status journal_status FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.id=? AND i.company_id=?").bind(invoiceId, actor.companyId).first<Record<string, string | number | null>>(); }, alreadyIssued: async (facts) => { const { invoiceId, actor } = facts; const write = await db(actor); return !!await write.prepare("SELECT 1 FROM finance_issued_thai_documents WHERE invoice_id=? AND document_kind='tax_invoice'").bind(invoiceId).first(); }, commitTaxInvoice: async (facts) => {
            const { actor, commandId, operation, input, invoiceId, issue } = facts;
            const write = await db(actor);
            const nativeIssue = issues.get(issue!.id)!;
            return executeCommand(write, actor, commandId, operation, input, invoiceId, input.expectedVersion, { id: issue.id, documentNumber: issue.documentNumber, sha256: issue.sha256, status: "issued" }, [
                assertion(write, commandId, "invoice-source", "EXISTS(SELECT 1 FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.id=? AND i.company_id=? AND i.version=? AND i.status IN ('posted','partially_paid','paid') AND j.status='posted')", [invoiceId, actor.companyId, input.expectedVersion]),
                assertion(write, commandId, "single-document", "NOT EXISTS(SELECT 1 FROM finance_issued_thai_documents WHERE invoice_id=? AND document_kind='tax_invoice')", [invoiceId]), ...nativeIssue!.steps,
            ]);
        }, document: async (facts) => { const { id, actor } = facts; const write = await db(actor); return write.prepare(ISSUED_THAI_DOCUMENT_BY_ID_SQL).bind(id, actor.companyId).first<IssuedDocument>(); }, events: async (facts) => { const { id, actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_thai_document_events WHERE document_id=? AND company_id=? ORDER BY created_at,id").bind(id, actor.companyId).all<FinanceRow>()).results; }, changes: async (facts) => { const { id, actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_thai_document_changes WHERE document_id=? AND company_id=? ORDER BY created_at,id").bind(id, actor.companyId).all<FinanceRow>()).results; }, identities: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT id,party_id,legal_name,tax_id,version FROM finance_tax_identities WHERE company_id=? AND status='approved' ORDER BY legal_name").bind(actor.companyId).all<FinanceRow>()).results; }, sourcePartyId: async (facts) => { const { document, actor } = facts; const write = await db(actor); return write.prepare(document.payment_id ? "SELECT party_id FROM finance_payments WHERE id=? AND company_id=?" : "SELECT party_id FROM finance_invoices WHERE id=? AND company_id=?").bind(document.payment_id ?? document.invoice_id, actor.companyId).first<string>("party_id"); }, exportDocument: async (facts) => {
            const { id, actor } = facts;
            const write = await db(actor);
            return write.prepare("SELECT export_html,export_sha256,document_number FROM finance_issued_thai_documents WHERE id=? AND company_id=?").bind(id, actor.companyId).first<{
                export_html: string;
                export_sha256: string;
                document_number: string;
            }>();
        }, originalForChange: async (facts) => { const { documentId, actor } = facts; const write = await db(actor); return write.prepare(ISSUED_THAI_DOCUMENT_BY_ID_SQL).bind(documentId, actor.companyId).first<IssuedDocument>(); }, sourcePartyForChange: async (facts) => {
            const { original, actor } = facts;
            const write = await db(actor);
            return write.prepare(original.payment_id ? "SELECT party_id FROM finance_payments WHERE id=? AND company_id=?" : "SELECT party_id FROM finance_invoices WHERE id=? AND company_id=?").bind(original.payment_id ?? original.invoice_id, actor.companyId).first<{
                party_id: string;
            }>();
        }, commitDocumentChange: async (facts) => {
            const { actor, commandId, input, id, payer, payee, documentId, source, now } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, commandId, "finance.thai-document.change.prepare", input, id, 1, { id, status: "draft", version: 1 }, [
                ...taxIdentityGuards(write, actor, commandId, [payer, payee]),
                assertion(write, commandId, "current-document", "NOT EXISTS(SELECT 1 FROM finance_thai_document_events WHERE document_id=? AND action IN ('cancelled','superseded'))", [documentId]),
                await nativeFinanceThaiDocuments(write, actor.companyId).changeInsert([id, actor.companyId, documentId, payer.id, payee.id, input.issueDate, input.reason, taxSourceDigest(source), JSON.stringify(source), actor.userId, now, now]),
            ]);
        }, pendingChange: async (facts) => { const { id, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_thai_document_changes WHERE id=? AND company_id=?").bind(id, actor.companyId).first<Record<string, string | number | null>>(); }, originalForDecision: async (facts) => { const { change, actor } = facts; const write = await db(actor); return write.prepare(ISSUED_THAI_DOCUMENT_BY_ID_SQL).bind(change.document_id, actor.companyId).first<IssuedDocument>(); }, financialSourceForDecision: async (facts) => {
            const { original, actor } = facts;
            const write = await db(actor);
            return write.prepare(original.payment_id ? "SELECT party_id,status FROM finance_payments WHERE id=? AND company_id=?" : "SELECT party_id,status FROM finance_invoices WHERE id=? AND company_id=?").bind(original.payment_id ?? original.invoice_id, actor.companyId).first<{
                party_id: string;
                status: string;
            }>();
        }, commitDocumentDecision: async (facts) => {
            const { actor, commandId, operation, input, id, status, issue, original, now } = facts;
            const write = await db(actor);
            const nativeIssue = issue ? issues.get(issue.id)! : null;
            const guards = [assertion(write, commandId, 'amendment', idCompanyStatusVersionGuard("finance_thai_document_changes", "draft"), [id, actor.companyId, input.expectedVersion]), assertion(write, commandId, 'current-document', "NOT EXISTS(SELECT 1 FROM finance_thai_document_events WHERE document_id=? AND action IN ('cancelled','superseded'))", [original.id])];
            if (issue)
                guards.push(assertion(write, commandId, 'financial-source', original.payment_id ? "EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND status='settled')" : INVOICE_EXISTS_PREFIX + " AND status IN ('posted','partially_paid','paid'))", [original.payment_id ?? original.invoice_id, actor.companyId]));
            return executeCommand(write, actor, commandId, operation, input, id, input.expectedVersion + 1, { id, status, version: input.expectedVersion + 1, replacementDocumentId: issue?.id ?? null }, [
                ...guards, ...(nativeIssue?.steps ?? []),
                ...(issue ? [await nativeFinanceThaiDocuments(write, actor.companyId).eventSupersede([`thai-supersede-${commandId}`, original.id, actor.companyId, issue.id, input.reason, actor.userId, commandId, now])] : []),
                await nativeFinanceThaiDocuments(write, actor.companyId).changeDecision([status, issue?.id ?? null, actor.userId, input.reason, now, now, id]),
            ]);
        } });
}
