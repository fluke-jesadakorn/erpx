import { financeSnapshotSources as pmsSnapshot } from "@erpx/server-pms/public/finance-snapshot-sources";
import { financeSnapshotSources as posSnapshot } from "@erpx/server-pos/public/finance-snapshot-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { Count } from "@erpx/server-pms/public/hospitality-day-control-contracts";
export function pmsDayControlFinanceFacts(db: SqlDatabase) {
    return {
        missingDirectInvoices: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => db.prepare(`SELECT count(*) n ${posSnapshot().completedSales}
      AND ${posSnapshot().linkedProperty} AND s.invoice_id IS NOT NULL AND (NOT EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=s.invoice_id AND i.company_id=s.company_id AND i.source_type='pos' AND i.source_id=s.id AND i.total_minor=s.total_minor AND i.tax_minor=s.tax_minor AND i.currency=s.currency AND i.status='paid')
        OR ${posSnapshot().missingPosting})`).bind(...facts.scope, facts.businessDate, facts.propertyId).first<Count>().then(row => Number(row?.n ?? 0)),
        unmatchedVerifiedTransfers: (facts: {
            propertyId: string;
            scope: [
                string,
                string
            ];
            businessDate: string;
        }) => db.prepare(`SELECT count(*) n ${posSnapshot().successfulTransfers}
      AND ${posSnapshot().linkedProperty} AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t WHERE t.company_id=s.company_id AND t.matched_type='payment'
        AND t.matched_id=p.finance_payment_id AND t.match_status='matched' AND t.external_id=p.reference
        AND t.amount_minor=p.amount_minor AND t.currency=s.currency)`).bind(...facts.scope, facts.businessDate, facts.propertyId).first<Count>().then(row => Number(row?.n ?? 0)),
        unmatchedVerifiedHotelTransfers: (facts: {
            companyId: string;
            propertyId: string;
            businessDate: string;
        }) => db.prepare(`SELECT count(*) n ${pmsSnapshot().verifiedTransfers}
    AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements b ON b.id=t.statement_id
      WHERE t.id=r.bank_transaction_id AND t.company_id=r.company_id AND t.match_status='matched' AND t.matched_type='payment'
        AND t.matched_id=r.finance_payment_id AND t.external_id=r.reference AND t.amount_minor=r.amount_minor AND t.currency=r.currency
        AND b.bank_account=(${pmsSnapshot().propertyBank}))`).bind(facts.companyId, facts.propertyId, facts.businessDate).first<Count>().then(row => Number(row?.n ?? 0)),
        stockLedger: (facts: {
            propertyId: string;
            companyId: string;
            businessDate: string;
            branchId: string;
            branchId2: string;
        }) => db.prepare(`SELECT COALESCE(SUM(l.debit_minor-l.credit_minor),0) n FROM finance_journal_lines l
    JOIN finance_journals j ON j.id=l.journal_id
    WHERE j.company_id=? AND j.entry_date=? AND j.source_type='pos' AND j.status='posted'
      AND l.dimension_json::jsonb->>'branchId'=?
      AND l.account_id IN (${posSnapshot().inventoryAccounts}) AND ${posSnapshot().linkedJournal}`).bind(facts.companyId, facts.businessDate, facts.branchId, facts.branchId2, facts.propertyId).first<Count>(),
        unbalancedJournals: (facts: {
            companyId: string;
            businessDate: string;
            branchId: string;
            propertyId: string;
        }) => db.prepare(`SELECT count(*) n FROM (
      SELECT j.id FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id
      WHERE j.company_id=? AND j.entry_date=? AND j.status='posted'
        AND ((j.source_type='pos' AND ${posSnapshot().linkedJournal} AND EXISTS(SELECT 1 FROM finance_journal_lines scoped WHERE scoped.journal_id=j.id AND scoped.dimension_json::jsonb->>'branchId'=?))
          OR ${pmsSnapshot().postedProperty})
      GROUP BY j.id HAVING SUM(l.debit_minor)<>SUM(l.credit_minor)
    ) unbalanced`).bind(facts.companyId, facts.businessDate, facts.propertyId, facts.branchId, facts.propertyId).first<Count>().then(row => Number(row?.n ?? 0))
    };
}
