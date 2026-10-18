import {withFinancePurchasedCommand,withFinancePurchasedRead} from "./purchase-boundary";
import { financeOwnerDecisionEffects } from "../../workflow/public/finance-owner-decision-effects";
import {financeTaxSourceVersion} from "../../commerce/public/finance-tax-source-version";
import {financeOwnedSnapshotSources as commerceSnapshotSources} from "../../commerce/public/finance-owned-snapshot-sources";
import { createVatRegister } from '../application/vat-register';
import { type VatTaxEntryPlan, monthDates, vatEntrySchema, taxReviewSchema, vatCorrectionSchema, vatPackageSchema, vatFilingSchema, type TaxRow, type VatSourceType, type VatCloseEvidence } from '../domain/vat-register';
export { vatEntrySchema, taxReviewSchema, vatCorrectionSchema, vatPackageSchema, vatFilingSchema } from '../domain/vat-register';
export type { TaxRow, VatCloseEvidence, VatCorrectionDetail } from '../domain/vat-register';
import "server-only";
import { createHash } from "node:crypto";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
const digest = (value: unknown) => createHash("sha256").update(JSON.stringify(value)).digest("hex");
export const pendingVatReviewSql = `WITH scope AS (
  SELECT company_id,statutory_chart_id,effective_date,?::text end_date FROM finance_accounting_profiles WHERE company_id=? AND status='approved'
) SELECT COUNT(*) value FROM (
  SELECT i.id FROM scope s JOIN finance_invoices i ON i.company_id=s.company_id JOIN finance_journals j ON j.id=i.journal_id
  WHERE j.chart_id=s.statutory_chart_id AND j.status IN ('posted','reversed') AND j.entry_date BETWEEN s.effective_date AND s.end_date AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=i.company_id AND t.source_type='invoice' AND t.source_id=i.id AND t.status='approved')
  UNION ALL SELECT r.id FROM scope s ${commerceSnapshotSources().finance_tax_register_commerce_returns_r_join} ON r.company_id=s.company_id JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_journals j ON j.id=r.credit_journal_id
  WHERE r.status='posted' AND j.chart_id=s.statutory_chart_id AND j.status='posted' AND j.entry_date BETWEEN s.effective_date AND s.end_date AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='commerce_return' AND t.source_id=r.id AND t.status='approved')
  UNION ALL SELECT r.id FROM scope s JOIN finance_billing_corrections r ON r.company_id=s.company_id JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_journals j ON j.id=r.credit_journal_id
  WHERE r.status='posted' AND j.chart_id=s.statutory_chart_id AND j.status='posted' AND j.entry_date BETWEEN s.effective_date AND s.end_date AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='billing_credit' AND t.source_id=r.id AND t.status='approved')
  UNION ALL SELECT c.id FROM scope s JOIN finance_vat_corrections c ON c.company_id=s.company_id AND c.chart_id=s.statutory_chart_id
    JOIN finance_tax_entries t ON t.id=c.root_entry_id WHERE c.status='prepared' AND t.accounting_date<=s.end_date
) pending`;
async function vatSourceRow(write: SqlDatabase, actor: WorkContext, sourceType: VatSourceType, id: string) {
    return write.prepare(sourceType === "invoice" ?
        `SELECT i.id,i.version,i.kind,i.invoice_number document_number,i.invoice_date document_date,i.currency,i.subtotal_minor,i.tax_minor,i.tax_account_id,i.journal_id,
      i.party_id,p.name party_name,p.tax_registration,p.version party_version,j.chart_id,j.entry_date accounting_date,j.status journal_status,j.currency journal_currency,c.base_currency,
      e.id financial_event_id,e.evidence_json financial_evidence
      FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id JOIN finance_journals j ON j.id=i.journal_id
      JOIN finance_charts c ON c.id=j.chart_id JOIN finance_financial_events e ON e.journal_id=j.id
      WHERE i.id=? AND i.company_id=?` :
        sourceType === 'billing_credit' ? `SELECT r.id,r.version,i.kind,r.number document_number,r.entry_date document_date,i.currency,r.subtotal_minor,r.tax_minor,i.tax_account_id,r.credit_journal_id journal_id,
      i.party_id,p.name party_name,p.tax_registration,p.version party_version,j.chart_id,j.entry_date accounting_date,j.status journal_status,j.currency journal_currency,c.base_currency,
      e.id financial_event_id,e.evidence_json financial_evidence,r.invoice_id
      FROM finance_billing_corrections r JOIN finance_invoices i ON i.id=r.invoice_id AND i.company_id=r.company_id JOIN finance_parties p ON p.id=i.party_id
      JOIN finance_journals j ON j.id=r.credit_journal_id JOIN finance_charts c ON c.id=j.chart_id JOIN finance_financial_events e ON e.journal_id=j.id
      WHERE r.id=? AND r.company_id=? AND r.status='posted'` :
            `SELECT r.id,r.version,i.kind,r.number document_number,r.return_date document_date,i.currency,r.subtotal_minor,r.tax_minor,i.tax_account_id,r.credit_journal_id journal_id,
      i.party_id,p.name party_name,p.tax_registration,p.version party_version,j.chart_id,j.entry_date accounting_date,j.status journal_status,j.currency journal_currency,c.base_currency,
      e.id financial_event_id,e.evidence_json financial_evidence,r.invoice_id
      ${commerceSnapshotSources().finance_tax_register_commerce_returns_r_from} JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_parties p ON p.id=i.party_id
      JOIN finance_journals j ON j.id=r.credit_journal_id JOIN finance_charts c ON c.id=j.chart_id JOIN finance_financial_events e ON e.journal_id=j.id
      WHERE r.id=? AND r.company_id=? AND r.status='posted'`).bind(id, actor.companyId).first<TaxRow>();
}
function sourceGuards(write: SqlDatabase, actor: WorkContext, key: string, type: "invoice" | "commerce_return" | "billing_credit", source: TaxRow, prefix = "") {
    const table = type === "invoice" ? "finance_invoices" : "finance_billing_corrections";
    return [
        type === "commerce_return" ? financeTaxSourceVersion(write).guard(key, `${prefix}tax-source-version`, source.id as string, actor.companyId, source.version as number) : assertion(write, key, `${prefix}tax-source-version`, `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`, [source.id, actor.companyId, source.version]),
        assertion(write, key, `${prefix}tax-party-version`, "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND version=?)", [source.party_id, actor.companyId, source.party_version]),
        assertion(write, key, `${prefix}statutory-profile`, "EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE company_id=? AND statutory_chart_id=? AND status='approved' AND effective_date<=?)", [actor.companyId, source.chart_id, source.accounting_date]),
    ];
}
function decision(write: SqlDatabase, actor: WorkContext, key: string, id: string, operation: string, version: number, input: unknown) {
    return financeOwnerDecisionEffects(write).appendRetainedLayoutDecision({id:crypto.randomUUID(),commandId:key,recordId:id,action:operation,actorId:actor.userId,assignmentId:actor.assignmentId,reviewedVersion:version,inputDigest:digest(input),createdAt:Date.now()});
}
function insertTaxEntry(write: SqlDatabase, actor: WorkContext, p: VatTaxEntryPlan) {
    return write.prepare(`INSERT INTO finance_tax_entries(id,company_id,chart_id,financial_event_id,tax_account_id,source_type,source_id,source_version,original_entry_id,direction,document_date,accounting_date,tax_point_date,claim_month,document_reference,treatment,deductible_bps,taxable_minor,tax_minor,deductible_minor,source_hash,evidence_json,prepared_by,created_at,replaces_entry_id,correction_id)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(p.id, actor.companyId, p.chartId, p.financialEventId, p.taxAccountId, p.sourceType, p.sourceId, p.sourceVersion, p.originalEntryId, p.direction, p.documentDate, p.accountingDate, p.taxPointDate, p.claimMonth, p.documentReference, p.treatment, p.deductibleBps, p.taxableMinor, p.taxMinor, p.deductibleMinor, p.sourceHash, p.evidenceJson, actor.userId, p.createdAt, p.replacesEntryId, p.correctionId);
}
async function originalVatEntry(write: SqlDatabase, actor: WorkContext, source: TaxRow) {
    return write.prepare("SELECT * FROM finance_current_tax_entries WHERE company_id=? AND source_type='invoice' AND source_id=? AND status='approved'").bind(actor.companyId, source.invoice_id).first<TaxRow>();
}
export async function prepareVatEntry(actor: WorkContext, raw: z.infer<typeof vatEntrySchema>, key: string) { return withFinancePurchasedCommand(actor,{key:key,operation:"finance.tax.prepare",purpose:"settle-existing",capability:"finance.tax"},tx=>vatService(tx).prepareVatEntry(actor, raw, key)); }
export async function reviewVatEntry(actor: WorkContext, id: string, action: "approve" | "reject", raw: z.infer<typeof taxReviewSchema>, key: string) { return withFinancePurchasedCommand(actor,{key:key,operation:`finance.tax.${action}`,purpose:"settle-existing",capability:"finance.tax"},tx=>vatService(tx).reviewVatEntry(actor, id, action, raw, key)); }
const familyEvidenceSql = `jsonb_build_object(
  'returns',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',r.id,'version',r.version,'status',r.status,'journal',r.credit_journal_id) ORDER BY r.id) ${commerceSnapshotSources().finance_tax_register_commerce_returns_r_from} WHERE r.company_id=? AND r.invoice_id=?),'[]'::jsonb),
  'billingCredits',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',r.id,'version',r.version,'status',r.status,'journal',r.credit_journal_id) ORDER BY r.id) FROM finance_billing_corrections r WHERE r.company_id=? AND r.invoice_id=?),'[]'::jsonb),
  'entries',COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM finance_current_tax_entries t WHERE t.company_id=? AND
    ((t.source_type='invoice' AND t.source_id=?) OR (t.source_type='commerce_return' AND t.source_id IN (SELECT id ${commerceSnapshotSources().finance_tax_register_commerce_returns_from} WHERE company_id=? AND invoice_id=?)) OR (t.source_type='billing_credit' AND t.source_id IN (SELECT id FROM finance_billing_corrections WHERE company_id=? AND invoice_id=?)))),'[]'::jsonb))::text`;
async function familyEvidence(write: SqlDatabase, actor: WorkContext, familyId: string) {
    return String(await write.prepare(`SELECT ${familyEvidenceSql} value`).bind(actor.companyId, familyId, actor.companyId, familyId, actor.companyId, familyId, actor.companyId, familyId, actor.companyId, familyId).first("value"));
}
function familyGuard(write: SqlDatabase, actor: WorkContext, key: string, familyId: string, json: string) {
    return assertion(write, key, "vat-correction-family", `(${familyEvidenceSql})::jsonb=?::jsonb`, [actor.companyId, familyId, actor.companyId, familyId, actor.companyId, familyId, actor.companyId, familyId, actor.companyId, familyId, json]);
}
export async function prepareVatCorrection(actor: WorkContext, entryId: string, raw: z.infer<typeof vatCorrectionSchema>, key: string) { return withFinancePurchasedCommand(actor,{key:key,operation:"finance.vat-correction.prepare",purpose:"settle-existing",capability:"finance.tax"},tx=>vatService(tx).prepareVatCorrection(actor, entryId, raw, key)); }
export async function reviewVatCorrection(actor: WorkContext, id: string, action: "approve" | "reject", raw: z.infer<typeof taxReviewSchema>, key: string) { return withFinancePurchasedCommand(actor,{key:key,operation:`finance.vat-correction.${action}`,purpose:"settle-existing",capability:"finance.tax"},tx=>vatService(tx).reviewVatCorrection(actor, id, action, raw, key)); }
// The hash covers the complete reviewed register for the month. List limits in
// the workspace do not truncate calculation or source-change checks.
const registerJson = `COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM finance_current_tax_entries t WHERE t.company_id=? AND t.chart_id=? AND t.claim_month=?),'[]'::jsonb)::text`;
const reconciliationSql = `WITH tax_accounts AS (
      SELECT DISTINCT i.tax_account_id id,CASE WHEN i.kind='receivable' THEN 'output' ELSE 'input' END direction
      FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.company_id=? AND j.chart_id=? AND j.entry_date BETWEEN ? AND ? AND i.tax_account_id IS NOT NULL
    ) SELECT a.id,a.direction,ac.code,ac.name,
      COALESCE((SELECT SUM(CASE WHEN a.direction='output' THEN l.base_credit_minor-l.base_debit_minor ELSE l.base_debit_minor-l.base_credit_minor END)
        FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.account_id=a.id AND j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date BETWEEN ? AND ?),0) ledger_minor,
      COALESCE((SELECT SUM(t.tax_minor) FROM finance_current_tax_entries t WHERE t.tax_account_id=a.id AND t.company_id=? AND t.chart_id=? AND t.direction=a.direction AND t.status='approved' AND t.accounting_date BETWEEN ? AND ?),0) register_minor,
      COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM finance_current_tax_entries t WHERE t.tax_account_id=a.id AND t.company_id=? AND t.chart_id=? AND t.direction=a.direction AND t.status='approved' AND t.accounting_date BETWEEN ? AND ?),'[]'::jsonb)::text register_evidence_json
      FROM tax_accounts a JOIN finance_accounts ac ON ac.id=a.id ORDER BY a.id,a.direction`;
const missingSql = `SELECT COUNT(*) FROM (SELECT i.id FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id
    WHERE i.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date BETWEEN ? AND ? AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=i.company_id AND t.source_type='invoice' AND t.source_id=i.id AND t.status='approved')
    UNION ALL SELECT r.id ${commerceSnapshotSources().finance_tax_register_commerce_returns_r_from} JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_journals j ON j.id=r.credit_journal_id
    WHERE r.company_id=? AND j.chart_id=? AND r.status='posted' AND j.status='posted' AND j.entry_date BETWEEN ? AND ? AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='commerce_return' AND t.source_id=r.id AND t.status='approved')
    UNION ALL SELECT r.id FROM finance_billing_corrections r JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_journals j ON j.id=r.credit_journal_id
    WHERE r.company_id=? AND j.chart_id=? AND r.status='posted' AND j.status='posted' AND j.entry_date BETWEEN ? AND ? AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='billing_credit' AND t.source_id=r.id AND t.status='approved')) pending`;
const pendingCorrectionsSql = "SELECT COUNT(*) FROM finance_vat_corrections c JOIN finance_tax_entries t ON t.id=c.root_entry_id WHERE c.company_id=? AND c.chart_id=? AND c.status='prepared' AND t.accounting_date<=?";
function reconciliationGuards(write: SqlDatabase, actor: WorkContext, key: string, chartId: string, endDate: string, effectiveDate: string, rows: TaxRow[]) {
    const args = [actor.companyId, chartId, effectiveDate, endDate];
    return [
        assertion(write, key, "vat-gl-evidence", `COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id,r.direction) FROM (${reconciliationSql}) r),'[]'::jsonb)=?::jsonb`, [...args, ...args, ...args, ...args, JSON.stringify(rows)]),
        assertion(write, key, "vat-missing-sources", `(${missingSql})=0`, [...args, ...args, ...args]),
        assertion(write, key, "vat-pending-corrections", `(${pendingCorrectionsSql})=0`, [actor.companyId, chartId, endDate]),
    ];
}
export async function calculateVatCloseEvidence(write: SqlDatabase, actor: WorkContext, endDate: string): Promise<VatCloseEvidence | null> { return vatService(write).calculateVatCloseEvidence(actor, endDate); }
export function vatCloseGuards(write: SqlDatabase, actor: WorkContext, key: string, endDate: string, evidence: VatCloseEvidence | null) {
    if (!evidence)
        return [assertion(write, key, "close-vat-profile-absent", "NOT EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE company_id=? AND status='approved' AND effective_date<=?)", [actor.companyId, endDate])];
    return [assertion(write, key, "close-vat-profile", "EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE id=? AND company_id=? AND statutory_chart_id=? AND status='approved' AND effective_date=?)", [evidence.profileId, actor.companyId, evidence.chartId, evidence.effectiveDate]),
        ...reconciliationGuards(write, actor, key, evidence.chartId, evidence.endDate, evidence.effectiveDate, evidence.rows)];
}
function registerGuard(write: SqlDatabase, actor: WorkContext, key: string, p: TaxRow, json: string) {
    return assertion(write, key, "vat-register", `(${registerJson})::jsonb=?::jsonb`, [actor.companyId, p.chart_id, p.filing_month, json]);
}
function packageEvent(write: SqlDatabase, actor: WorkContext, key: string, id: string, version: number, action: string, evidence: unknown) {
    return write.prepare("INSERT INTO finance_statutory_package_events VALUES(?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(), id, key, version, action, actor.userId, JSON.stringify(evidence), Date.now());
}
export async function prepareVatPackage(actor: WorkContext, raw: z.infer<typeof vatPackageSchema>, key: string) { return withFinancePurchasedCommand(actor,{key:key,operation:"finance.vat-package.prepare",purpose:"settle-existing",capability:"finance.tax"},tx=>vatService(tx).prepareVatPackage(actor, raw, key)); }
export async function approveVatPackage(actor: WorkContext, id: string, raw: z.infer<typeof taxReviewSchema>, key: string) { return withFinancePurchasedCommand(actor,{key:key,operation:"finance.vat-package.approve",purpose:"settle-existing",capability:"finance.tax"},tx=>vatService(tx).approveVatPackage(actor, id, raw, key)); }
export async function exportVatWorkpaper(actor: WorkContext, id: string, raw: z.infer<typeof taxReviewSchema>, key: string) { return withFinancePurchasedCommand(actor,{key:key,operation:"finance.vat-package.export",purpose:"settle-existing",capability:"finance.tax"},tx=>vatService(tx).exportVatWorkpaper(actor, id, raw, key)); }
export async function recordVatFiling(actor: WorkContext, id: string, raw: z.infer<typeof vatFilingSchema>, key: string) { return withFinancePurchasedCommand(actor,{key:key,operation:"finance.vat-package.file",purpose:"settle-existing",capability:"finance.tax"},tx=>vatService(tx).recordVatFiling(actor, id, raw, key)); }
export async function supersedeVatPackage(actor: WorkContext, id: string, raw: z.infer<typeof taxReviewSchema>, key: string) { return withFinancePurchasedCommand(actor,{key:key,operation:"finance.vat-package.supersede",purpose:"settle-existing",capability:"finance.tax"},tx=>vatService(tx).supersedeVatPackage(actor, id, raw, key)); }
export async function vatWorkspace(actor: WorkContext, query: {
    chartId?: string;
    filingMonth?: string;
} = {}) { return withFinancePurchasedRead(actor,db=>vatService(db).vatWorkspace(actor, query),"finance.tax"); }
export async function vatCorrectionDetail(actor: WorkContext, id: string) { return withFinancePurchasedRead(actor,db=>vatService(db).vatCorrectionDetail(actor, id),"finance.tax"); }
export async function taxEntryDetail(actor: WorkContext, id: string) { return withFinancePurchasedRead(actor,db=>vatService(db).taxEntryDetail(actor, id),"finance.tax"); }
export async function vatPackageDetail(actor: WorkContext, id: string) { return withFinancePurchasedRead(actor,db=>vatService(db).vatPackageDetail(actor, id),"finance.tax"); }
export async function vatPackageExport(actor: WorkContext, id: string) { return withFinancePurchasedRead(actor,db=>vatService(db).vatPackageExport(actor, id),"finance.tax"); }
function vatService(provided?: SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined;
    const db = (actor: WorkContext) => provided ? Promise.resolve(provided) : pending ??= holdingResources(actor.holdingId).then(r => r.write);
    return createVatRegister({ error: (...args) => new ApiError(...args), permission: requirePermission, ready: async (actor) => { await db(actor); }, now: Date.now, digest, hashBytes: value => createHash('sha256').update(value).digest('hex'), replay: async <Result>(actor: WorkContext, key: string, operation: string, id: string, input: unknown) => replayCommand<Result>(await db(actor), actor, key, operation, id, input), vatSource: async (actor, type, id) => vatSourceRow(await db(actor), actor, type, id), profile: async (actor, chartId, date) => (await db(actor)).prepare("SELECT * FROM finance_accounting_profiles WHERE company_id=? AND statutory_chart_id=? AND status='approved' AND effective_date<=?").bind(actor.companyId, chartId, date).first<TaxRow>(), package: async (actor, id) => (await db(actor)).prepare("SELECT * FROM finance_statutory_packages WHERE id=? AND company_id=? AND form_code='PP30' AND filing_month IS NOT NULL").bind(id, actor.companyId).first<TaxRow>(), correction: async (actor, id) => (await db(actor)).prepare("SELECT * FROM finance_vat_corrections WHERE id=? AND company_id=?").bind(id, actor.companyId).first<TaxRow>(), originalVatEntry: async (actor, source) => originalVatEntry(await db(actor), actor, source), familyEvidence: async (actor, id) => familyEvidence(await db(actor), actor, id), fullRegister: async (actor, chartId, filingMonth) => (await db(actor)).prepare(`SELECT ${registerJson} value`).bind(actor.companyId, chartId, filingMonth).first<string>('value'), reconciliationRows: async (actor, chartId, start, end) => { const args = [actor.companyId, chartId, start, end]; return (await (await db(actor)).prepare(reconciliationSql).bind(...args, ...args, ...args, ...args).all<TaxRow>()).results; }, missingSources: async (actor, chartId, start, end) => { const args = [actor.companyId, chartId, start, end]; return (await db(actor)).prepare(missingSql).bind(...args, ...args, ...args).first<number>('count'); }, pendingCorrectionCount: async (actor, chartId, end) => (await db(actor)).prepare(pendingCorrectionsSql).bind(actor.companyId, chartId, end).first<number>('count'), commitPrepareVatEntry: async (facts) => {
            const { actor, key, input, id, ledger, deductible, source, familyId, taxEntry } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, "finance.tax.prepare", input, id, 1, { id, status: "prepared", version: 1, taxMinor: ledger, deductibleMinor: deductible }, [
                ...sourceGuards(write, actor, key, input.sourceType, source),
                assertion(write, key, "tax-family-correction", "NOT EXISTS(SELECT 1 FROM finance_vat_corrections WHERE company_id=? AND family_source_id=? AND status='prepared')", [actor.companyId, familyId]),
                insertTaxEntry(write, actor, taxEntry),
            ]);
        }, entry: async (facts) => { const { id, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_tax_entries WHERE id=? AND company_id=?").bind(id, actor.companyId).first<TaxRow>(); }, commitReviewVatEntry: async (facts) => {
            const { actor, key, operation, input, id, status, verifiedSource, entry } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, operation, input, id, input.expectedVersion + 1, { id, status, version: input.expectedVersion + 1 }, [
                assertion(write, key, "tax-entry", "EXISTS(SELECT 1 FROM finance_tax_entries WHERE id=? AND company_id=? AND version=? AND status='prepared')", [id, actor.companyId, input.expectedVersion]), ...(verifiedSource ? sourceGuards(write, actor, key, entry.source_type as VatSourceType, verifiedSource.source) : []),
                write.prepare("UPDATE finance_tax_entries SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE id=?").bind(status, actor.userId, input.reason, Date.now(), id),
                decision(write, actor, key, id, operation, input.expectedVersion, input),
            ]);
        }, previousEntry: async (facts) => { const { entryId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_current_tax_entries WHERE id=? AND company_id=? AND status='approved'").bind(entryId, actor.companyId).first<TaxRow>(); }, pendingFamily: async (facts) => { const { actor, familyId } = facts; const write = await db(actor); return write.prepare("SELECT 1 FROM finance_vat_corrections WHERE company_id=? AND family_source_id=? AND status='prepared'").bind(actor.companyId, familyId).first(); }, commitPrepareVatCorrection: async (facts) => {
            const { actor, key, operation, entryId, input, id, members, taxEntries, familyId, json, previous } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, operation, { entryId, ...input }, id, 1, { id, status: "prepared", version: 1, entryCount: members.length }, [
                familyGuard(write, actor, key, familyId, json),
                assertion(write, key, "vat-correction-current", "EXISTS(SELECT 1 FROM finance_current_tax_entries WHERE id=? AND company_id=? AND version=? AND status='approved')", [entryId, actor.companyId, input.expectedEntryVersion]),
                assertion(write, key, "vat-correction-volume", "(SELECT COUNT(*) FROM finance_vat_corrections WHERE company_id=? AND status='prepared')<100", [actor.companyId]),
                write.prepare("INSERT INTO finance_vat_corrections(id,company_id,chart_id,root_entry_id,family_source_id,reason,family_evidence_json,prepared_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(id, actor.companyId, previous.chart_id, entryId, familyId, input.reason, json, actor.userId, Date.now()),
                ...members.flatMap((member, index) => [
                    ...sourceGuards(write, actor, key, member.facts.sourceType, member.current.source, `replacement-${index}-`),
                    insertTaxEntry(write, actor, taxEntries[index]),
                ]),
            ]);
        }, replacementEntries: async (facts) => { const { id, actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_tax_entries WHERE correction_id=? ORDER BY id").bind(id).all<TaxRow>()).results; }, commitReviewVatCorrection: async (facts) => {
            const { actor, key, operation, input, id, status, entries, verifiedFamilyJson, correction, verifiedSources, now } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, operation, input, id, input.expectedVersion + 1, { id, status, version: input.expectedVersion + 1, entryCount: entries.length }, [
                assertion(write, key, "vat-correction-version", "EXISTS(SELECT 1 FROM finance_vat_corrections WHERE id=? AND company_id=? AND status='prepared' AND version=?)", [id, actor.companyId, input.expectedVersion]),
                assertion(write, key, "vat-correction-entries", "COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM finance_tax_entries t WHERE correction_id=?),'[]'::jsonb)=?::jsonb", [id, JSON.stringify(entries)]), ...(verifiedFamilyJson !== null ? [familyGuard(write, actor, key, String(correction.family_source_id), verifiedFamilyJson), ...verifiedSources.flatMap((current, index) => sourceGuards(write, actor, key, entries[index].source_type as VatSourceType, current.source, `replacement-${index}-`))] : []),
                write.prepare("UPDATE finance_tax_entries SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE correction_id=? AND status='prepared'").bind(status, actor.userId, input.reason, now, id),
                write.prepare("UPDATE finance_vat_corrections SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE id=?").bind(status, actor.userId, input.reason, now, id),
                decision(write, actor, key, id, operation, input.expectedVersion, input),
            ]);
        }, commitPrepareVatPackage: async (facts) => {
            const { actor, key, operation, input, id, result, r, dates, p, reconciliation, runId, now, snapshot, original } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, operation, input, id, 1, { id, status: "prepared", version: 1, result }, [
                registerGuard(write, actor, key, { chart_id: input.chartId, filing_month: input.filingMonth }, r.json),
                ...reconciliationGuards(write, actor, key, input.chartId, dates.end, String(p.effective_date), reconciliation.rows),
                assertion(write, key, "vat-profile", "EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE id=? AND company_id=? AND status='approved')", [p.id, actor.companyId]),
                write.prepare("INSERT INTO finance_calculation_runs(id,company_id,holding_id,kind,policy_version,currency,currency_scale,source_hash,input_json,result_json,explanation_json,status,prepared_by,created_at,updated_at) VALUES(?,?,?,'thai_statutory',1,'THB',2,?,?,?,?,'prepared',?,?,?)")
                    .bind(runId, actor.companyId, actor.holdingId, r.sourceHash, JSON.stringify({ registerEntryIds: r.entries.map(e => e.id) }), JSON.stringify(result), JSON.stringify({ engine: "source-vat-register-v1", preparationOnly: true, priorCreditsIncluded: false }), actor.userId, now, now),
                write.prepare("INSERT INTO finance_statutory_packages(id,company_id,jurisdiction,form_code,filing_kind,calculation_run_id,status,schema_version,effective_date,prepared_by,created_at,updated_at,chart_id,filing_month,source_hash,snapshot_json,original_package_id,evidence_json) VALUES(?,?,'TH','PP30',?,?,'prepared','erpx-pp30-workpaper-v1',?,?,?,?,?,?,?,?,?,?)")
                    .bind(id, actor.companyId, input.filingKind, runId, dates.end, actor.userId, now, now, input.chartId, input.filingMonth, r.sourceHash, JSON.stringify(snapshot), original?.id ?? null, JSON.stringify({ preparationReason: input.reason })),
                packageEvent(write, actor, key, id, 1, "prepare", { sourceHash: r.sourceHash, reason: input.reason }),
            ]);
        }, commitApproveVatPackage: async (facts) => {
            const { actor, key, operation, input, id, p, r, currentProfile, current } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, operation, input, id, input.expectedVersion + 1, { id, status: "approved_for_export", version: input.expectedVersion + 1 }, [
                assertion(write, key, "vat-package", "EXISTS(SELECT 1 FROM finance_statutory_packages WHERE id=? AND company_id=? AND status='prepared' AND version=?)", [id, actor.companyId, input.expectedVersion]),
                assertion(write, key, "vat-calculation", "EXISTS(SELECT 1 FROM finance_calculation_runs WHERE id=? AND company_id=? AND status='prepared' AND version=1)", [p.calculation_run_id, actor.companyId]),
                registerGuard(write, actor, key, p, r.json),
                ...reconciliationGuards(write, actor, key, String(p.chart_id), String(p.effective_date), String(currentProfile.effective_date), current.rows),
                write.prepare("UPDATE finance_statutory_packages SET status='approved_for_export',version=version+1,reviewed_by=?,approved_by=?,review_reason=?,updated_at=? WHERE id=?").bind(actor.userId, actor.userId, input.reason, Date.now(), id),
                write.prepare("UPDATE finance_calculation_runs SET status='approved',version=version+1,reviewed_by=?,approved_by=?,updated_at=? WHERE id=? AND status='prepared'").bind(actor.userId, actor.userId, Date.now(), p.calculation_run_id),
                decision(write, actor, key, id, operation, input.expectedVersion, input), packageEvent(write, actor, key, id, input.expectedVersion + 1, "approve", input),
            ]);
        }, commitExportVatWorkpaper: async (facts) => {
            const { actor, key, operation, input, id, p, content, exportHash, r, currentProfile, current } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, operation, input, id, input.expectedVersion + 1, { id, status: "exported", version: input.expectedVersion + 1, filename: `ERPX-PP30-workpaper-${p.filing_month}.csv`, content, exportHash }, [
                assertion(write, key, "vat-package", "EXISTS(SELECT 1 FROM finance_statutory_packages WHERE id=? AND company_id=? AND status='approved_for_export' AND version=?)", [id, actor.companyId, input.expectedVersion]), registerGuard(write, actor, key, p, r.json),
                ...reconciliationGuards(write, actor, key, String(p.chart_id), String(p.effective_date), String(currentProfile.effective_date), current.rows),
                write.prepare("UPDATE finance_statutory_packages SET status='exported',version=version+1,export_content=?,export_hash=?,updated_at=? WHERE id=?").bind(content, exportHash, Date.now(), id),
                packageEvent(write, actor, key, id, input.expectedVersion + 1, "export", { exportHash, reason: input.reason }),
            ]);
        }, commitRecordVatFiling: async (facts) => {
            const { actor, key, operation, input, id } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, operation, input, id, input.expectedVersion + 1, { id, status: "filed", version: input.expectedVersion + 1 }, [
                assertion(write, key, "vat-package", "EXISTS(SELECT 1 FROM finance_statutory_packages WHERE id=? AND company_id=? AND status='exported' AND version=?)", [id, actor.companyId, input.expectedVersion]),
                write.prepare("UPDATE finance_statutory_packages SET status='filed',version=version+1,filing_reference=?,filed_at=?,evidence_json=(evidence_json::jsonb||?::jsonb)::text,updated_at=? WHERE id=?").bind(input.filingReference, Date.parse(`${input.filedDate}T00:00:00+07:00`), JSON.stringify({ filing: input }), Date.now(), id),
                packageEvent(write, actor, key, id, input.expectedVersion + 1, "file", input),
            ]);
        }, commitSupersedeVatPackage: async (facts) => {
            const { actor, key, operation, input, id } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, operation, input, id, input.expectedVersion + 1, { id, status: "superseded", version: input.expectedVersion + 1 }, [
                assertion(write, key, "vat-package", "EXISTS(SELECT 1 FROM finance_statutory_packages WHERE id=? AND company_id=? AND version=? AND status IN ('prepared','approved_for_export','exported'))", [id, actor.companyId, input.expectedVersion]),
                write.prepare("UPDATE finance_statutory_packages SET status='superseded',version=version+1,updated_at=? WHERE id=?").bind(Date.now(), id), packageEvent(write, actor, key, id, input.expectedVersion + 1, "supersede", input),
            ]);
        }, profiles: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_accounting_profiles WHERE company_id=? AND status='approved'").bind(actor.companyId).all<TaxRow>()).results; }, chart: async (facts) => { const { chartId, actor } = facts; const write = await db(actor); return write.prepare("SELECT 1 FROM finance_charts WHERE id=? AND company_id=?").bind(chartId, actor.companyId).first(); }, workspaceEntries: async (facts) => { const { actor, chartId, filingMonth } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_current_tax_entries WHERE company_id=? AND chart_id=? AND claim_month=? ORDER BY tax_point_date,id LIMIT 1000").bind(actor.companyId, chartId, filingMonth).all<TaxRow>()).results; }, workspaceCandidates: async (facts) => {
            const { actor, chartId } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT * FROM (SELECT i.id,i.version,i.invoice_number,i.invoice_date,i.kind,i.subtotal_minor,i.tax_minor,j.entry_date accounting_date,p.name party_name,'invoice' source_type
      FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id JOIN finance_parties p ON p.id=i.party_id
      WHERE i.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND i.currency='THB' AND i.tax_account_id IS NOT NULL
      AND NOT EXISTS(SELECT 1 FROM finance_tax_entries t WHERE t.company_id=i.company_id AND t.source_type='invoice' AND t.source_id=i.id AND t.status<>'rejected')
      UNION ALL SELECT r.id,r.version,r.number,r.return_date,i.kind,-r.subtotal_minor,-r.tax_minor,j.entry_date,p.name,'commerce_return'
      ${commerceSnapshotSources().finance_tax_register_commerce_returns_r_from} JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_parties p ON p.id=i.party_id JOIN finance_journals j ON j.id=r.credit_journal_id
      WHERE r.company_id=? AND j.chart_id=? AND r.status='posted' AND j.status='posted' AND i.currency='THB' AND i.tax_account_id IS NOT NULL
      AND NOT EXISTS(SELECT 1 FROM finance_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='commerce_return' AND t.source_id=r.id AND t.status<>'rejected')
      UNION ALL SELECT r.id,r.version,r.number,r.entry_date,i.kind,-r.subtotal_minor,-r.tax_minor,j.entry_date,p.name,'billing_credit'
      FROM finance_billing_corrections r JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_parties p ON p.id=i.party_id JOIN finance_journals j ON j.id=r.credit_journal_id
      WHERE r.company_id=? AND j.chart_id=? AND r.status='posted' AND j.status='posted' AND i.currency='THB' AND i.tax_account_id IS NOT NULL
      AND NOT EXISTS(SELECT 1 FROM finance_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='billing_credit' AND t.source_id=r.id AND t.status<>'rejected')) candidates
      ORDER BY accounting_date,id LIMIT 100`).bind(actor.companyId, chartId, actor.companyId, chartId, actor.companyId, chartId).all<TaxRow>()).results;
        }, workspacePackages: async (facts) => { const { actor, chartId, filingMonth } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_statutory_packages WHERE company_id=? AND chart_id=? AND filing_month=? AND form_code='PP30' ORDER BY created_at DESC LIMIT 50").bind(actor.companyId, chartId, filingMonth).all<TaxRow>()).results; }, workspaceCorrections: async (facts) => { const { actor, chartId } = facts; const write = await db(actor); return (await write.prepare("SELECT c.*,t.document_reference FROM finance_vat_corrections c JOIN finance_tax_entries t ON t.id=c.root_entry_id WHERE c.company_id=? AND c.chart_id=? ORDER BY (c.status='prepared') DESC,c.created_at DESC,c.id LIMIT 100").bind(actor.companyId, chartId).all<TaxRow>()).results; }, correctionDetailRows: async (facts) => { const { id, actor, correction } = facts; const write = await db(actor); return (await write.prepare("SELECT t.*,to_jsonb(p)::text previous_json FROM finance_tax_entries t JOIN finance_tax_entries p ON p.id=t.replaces_entry_id WHERE t.correction_id=? AND t.company_id=? ORDER BY (t.replaces_entry_id=?) DESC,t.id").bind(id, actor.companyId, correction.root_entry_id).all<TaxRow>()).results; }, entryHistory: async (facts) => { const { actor, entry } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_tax_entries WHERE company_id=? AND source_type=? AND source_id=? ORDER BY created_at,id LIMIT 1000").bind(actor.companyId, entry.source_type, entry.source_id).all<TaxRow>()).results; }, currentEntry: async (facts) => { const { actor, entry } = facts; const write = await db(actor); return write.prepare("SELECT id FROM finance_current_tax_entries WHERE company_id=? AND source_type=? AND source_id=? AND status='approved'").bind(actor.companyId, entry.source_type, entry.source_id).first<TaxRow>(); }, sourceVersion: async (facts) => { const { entry, actor } = facts; const write = await db(actor); return entry.source_type === "commerce_return" ? financeTaxSourceVersion(write).read(String(entry.source_id),actor.companyId) : write.prepare(`SELECT version FROM ${entry.source_type === "invoice" ? "finance_invoices" : "finance_billing_corrections"} WHERE id=? AND company_id=?`).bind(entry.source_id, actor.companyId).first<TaxRow>(); }, packageEvents: async (facts) => { const { id, actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_statutory_package_events WHERE package_id=? ORDER BY version").bind(id).all<TaxRow>()).results; }, registerJson: async (facts) => { const { actor, p } = facts; const write = await db(actor); return write.prepare(`SELECT ${registerJson} value`).bind(actor.companyId, p.chart_id, p.filing_month).first<string>("value"); }, pendingCorrections: async (facts) => { const { actor, p } = facts; const write = await db(actor); return write.prepare(pendingCorrectionsSql).bind(actor.companyId, p.chart_id, monthDates(String(p.filing_month)).end).first<number>("count"); }, closeProfile: async (facts) => { const { actor, endDate } = facts; const write = await db(actor); return write.prepare("SELECT id,statutory_chart_id,effective_date FROM finance_accounting_profiles WHERE company_id=? AND status='approved' AND effective_date<=?").bind(actor.companyId, endDate).first<TaxRow>(); } });
}
