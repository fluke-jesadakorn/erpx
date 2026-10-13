import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { ThaiDocumentSnapshot } from "@/lib/finance/finance-thai-document";
import { minorToDecimal } from "@/lib/finance/finance-money";
import type { Row, SourceRow, Source } from '../domain/withholding-month';
import type { FinanceError } from '../domain/invoice-line-revision';
export interface WithholdingMonthSourcePorts {
    error: FinanceError;
    monthlyRows(actor: WorkContext, month: string, formKind: 'PND3' | 'PND53'): Promise<Row[]>;
    certificate(actor: WorkContext, id: unknown): Promise<{
        snapshot_json: string;
        document_number: string;
        export_sha256: string;
    } | null>;
    postedLiability(journalId: unknown, liabilityAccountId: string): Promise<number | string | null>;
    bankMovement(actor: WorkContext, id: string): Promise<Row | null>;
    openPeriods(actor: WorkContext, date: unknown): Promise<Array<{
        id: string;
    }>>;
}
export function createWithholdingMonthSource(ports: WithholdingMonthSourcePorts) {
    async function capture(actor: WorkContext, month: string, formKind: "PND3" | "PND53"): Promise<Source> {
        const rows = await ports.monthlyRows(actor, month, formKind);
        const sources: SourceRow[] = [];
        for (const row of rows) {
            const original = JSON.parse(String(row.source_snapshot_json)) as {
                chartId: string;
                liabilityAccountId: string;
            }, document = await ports.certificate(actor, row.document_id);
            if (!document)
                throw ports.error(409, "WITHHOLDING_CERTIFICATE_MISSING", "Every actual withheld payment needs its current retained certificate before monthly review.");
            const snapshot = JSON.parse(document.snapshot_json) as ThaiDocumentSnapshot, taxMinor = Number(row.tax_minor), posted = Number(await ports.postedLiability(row.journal_id, original.liabilityAccountId));
            if (posted !== taxMinor || snapshot.taxMinor !== taxMinor || snapshot.source.assessmentId !== row.id)
                throw ports.error(409, "WITHHOLDING_LIABILITY_CHANGED", "The actual payment liability and retained tax certificate must reconcile exactly.");
            // Signatures stay in their immutable source certificate. The workpaper records
            // the original certificate hash and the legal fields needed for manual filing.
            const payer = { ...snapshot.payer, signature_data_uri: null };
            sources.push({ assessmentId: String(row.id), assessmentVersion: Number(row.version), paymentId: String(row.payment_id), paymentVersion: Number(row.payment_version), paymentDate: String(row.confirmed_date), paymentJournalId: String(row.journal_id), liabilityAccountId: original.liabilityAccountId, chartId: original.chartId, taxMinor, incomeMinor: Number(row.income_minor), sourceFingerprint: String(row.source_fingerprint), documentId: String(row.document_id), documentNumber: document.document_number, documentSha256: document.export_sha256, payer, payee: snapshot.payee, items: snapshot.items, alreadyRemitted: Boolean(row.already_remitted) });
        }
        if (!sources.length || new Set(sources.map(row => row.chartId)).size !== 1)
            throw ports.error(409, "WITHHOLDING_MONTH_NOT_READY", "Review a nonempty ordinary monthly source set on one statutory THB chart.");
        const taxMinor = sources.filter(row => !row.alreadyRemitted).reduce((sum, row) => sum + row.taxMinor, 0), monthlyTaxMinor = sources.reduce((sum, row) => sum + row.taxMinor, 0);
        if (!Number.isSafeInteger(monthlyTaxMinor) || taxMinor <= 0)
            throw ports.error(409, "WITHHOLDING_MONTH_ALREADY_REMITTED", "Every current monthly tax source has already been remitted; no duplicate payment is due.");
        return { schema: "finance.withholding.month.v1", companyId: actor.companyId, month, formKind, sources, taxMinor, monthlyTaxMinor, chartId: sources[0].chartId, certified: false, filingAdapter: "manual_review_workpaper" };
    }
    async function bankEvidence(actor: WorkContext, input: {
        bankTransactionId: string;
        bankVersion: number;
        bankReference: string;
    }, amount: number, chartId: string, earliest: string, bankAccountId?: string) {
        const row = await ports.bankMovement(actor, input.bankTransactionId);
        if (!row || Number(row.version) !== input.bankVersion || !["unmatched", "proposed"].includes(String(row.match_status)) || Number(row.amount_minor) !== amount || row.external_id !== input.bankReference || row.currency !== "THB" || row.base_currency !== "THB" || row.control_kind !== "bank" || row.account_status !== "active" || row.chart_id !== chartId || String(row.transaction_date) < earliest || bankAccountId && row.bank_account !== bankAccountId)
            throw ports.error(422, "BANK_EVIDENCE_MISMATCH", "Use the exact current THB tax bank movement, account, amount, reference and date.");
        const periods = await ports.openPeriods(actor, row.transaction_date);
        if (periods.length !== 1)
            throw ports.error(409, "WITHHOLDING_REMITTANCE_PERIOD_REQUIRED", "One open accounting period must contain the actual tax bank date.");
        return { row, periodId: periods[0].id };
    }
    return { capture, bankEvidence, exportCsv };
}
export function exportCsv(source: Source) { const cell = (v: unknown) => `"${String(v ?? "").replaceAll('"', '""')}"`; return ["adapter,form,month,certificate,payment_date,payer_tax_id,payer_branch,payee_name,payee_tax_id,payee_branch,income_category,income_thb,tax_thb,certificate_sha256,prior_remittance", ...source.sources.flatMap(row => row.items.map(item => [source.filingAdapter, source.formKind, source.month, row.documentNumber, row.paymentDate, row.payer.tax_id, row.payer.branch_code, row.payee.legal_name, row.payee.tax_id, row.payee.branch_code, item.category, minorToDecimal(item.incomeMinor), minorToDecimal(item.taxMinor), row.documentSha256, row.alreadyRemitted ? "confirmed" : "new"].map(cell).join(",")))].join("\n"); }
