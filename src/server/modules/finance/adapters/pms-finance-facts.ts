import {financeOwnedSnapshotSources as pmsSnapshotSources} from "../../pms/public/finance-owned-snapshot-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function pmsFinancialOverviewFacts(db: SqlDatabase) {
    return {
        ledger: (facts: {
            companyId: string;
            branchId: string | null;
            branchId2: string | null;
            propertyId: string | null;
            propertyId2: string | null;
        }) => db.prepare(`SELECT p.id,
      COALESCE(sum(l.debit_minor-l.credit_minor) FILTER (WHERE l.account_id=p.config_json::jsonb->'accounts'->>'receivable'),0) receivable,
      COALESCE(sum(l.credit_minor-l.debit_minor) FILTER (WHERE l.account_id=p.config_json::jsonb->'accounts'->>'advance'),0) advance,
      COALESCE(sum(l.credit_minor-l.debit_minor) FILTER (WHERE l.account_id=p.config_json::jsonb->'accounts'->>'deposit'),0) deposit,
      COALESCE(sum(l.credit_minor-l.debit_minor) FILTER (WHERE l.account_id=p.config_json::jsonb->'onlineAccounting'->>'refundPayable'),0) refund_payable,
      COALESCE(sum(l.credit_minor-l.debit_minor) FILTER (WHERE l.account_id=p.config_json::jsonb->'onlineAccounting'->>'unallocated'),0) unallocated,
      COALESCE(sum(l.debit_minor-l.credit_minor) FILTER (WHERE l.account_id IN (p.config_json::jsonb->'accounts'->>'bank',p.config_json::jsonb->'accounts'->>'cash')),0) received,
      COALESCE(sum(l.debit_minor),0) debit,COALESCE(sum(l.credit_minor),0) credit
    ${pmsSnapshotSources().pms_finance_facts_pms_properties_p_from} LEFT JOIN (SELECT property_id,journal_id ${pmsSnapshotSources().pms_finance_facts_pms_postings_from}
      UNION SELECT property_id,opening_journal_id ${pmsSnapshotSources().pms_finance_facts_pms_import_batches_from} WHERE state='applied' AND opening_journal_id IS NOT NULL) pp ON pp.property_id=p.id
    LEFT JOIN finance_journals j ON j.id=pp.journal_id AND j.company_id=p.company_id AND j.status='posted'
    LEFT JOIN finance_journal_lines l ON l.journal_id=j.id
    WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?) GROUP BY p.id,p.config_json`).bind(facts.companyId, facts.branchId, facts.branchId2, facts.propertyId, facts.propertyId2).all<{
            id: string;
            receivable: number;
            advance: number;
            deposit: number;
            refund_payable: number;
            unallocated: number;
            received: number;
            debit: number;
            credit: number;
        }>(),
        exceptions: (facts: {
            companyId: string;
            branchId: string | null;
            branchId2: string | null;
            propertyId: string | null;
            propertyId2: string | null;
        }) => db.prepare(`SELECT r.id,r.property_id,r.data_json::jsonb->>'folioId' folio_id,
      r.data_json::jsonb->>'description' description,r.data_json::jsonb->>'kind' kind,
      (r.data_json::jsonb->>'total')::bigint total
    ${pmsSnapshotSources().pms_finance_facts_pms_records_r_from} ${pmsSnapshotSources().pms_finance_facts_pms_properties_p_join} ON p.id=r.property_id WHERE p.company_id=?
    AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?) AND r.kind='entry'
    AND (r.data_json::jsonb->>'total')::bigint<>0
    AND NOT EXISTS(SELECT 1 ${pmsSnapshotSources().pms_finance_facts_pms_import_balances_ib_from} ${pmsSnapshotSources().pms_finance_facts_pms_import_batches_b_join} ON b.id=ib.batch_id
      JOIN finance_journals j ON j.id=b.opening_journal_id AND j.status='posted'
      WHERE ib.entry_id=r.id AND ib.property_id=r.property_id AND b.state='applied')
    AND NOT EXISTS(SELECT 1 ${pmsSnapshotSources().pms_finance_facts_pms_postings_pp_from} JOIN finance_journals j ON j.id=pp.journal_id AND j.status='posted'
      WHERE pp.property_id=r.property_id AND pp.record_id=r.id)
    ORDER BY r.updated_at DESC,r.id LIMIT 100`).bind(facts.companyId, facts.branchId, facts.branchId2, facts.propertyId, facts.propertyId2).all<{
            id: string;
            property_id: string;
            folio_id: string;
            description: string;
            kind: string;
            total: number;
        }>(),
        documents: (facts: {
            companyId: string;
            branchId: string | null;
            branchId2: string | null;
            propertyId: string | null;
            propertyId2: string | null;
        }) => db.prepare(`SELECT d.id,d.property_id,d.folio_id,d.kind,d.finance_id,d.created_at,
      COALESCE(i.journal_id,pay.journal_id,d.snapshot_json::jsonb->>'openingJournalId') journal_id,
      CASE WHEN pay.id IS NOT NULL THEN 'payments' ELSE 'invoices' END finance_kind
    ${pmsSnapshotSources().pms_finance_facts_pms_documents_d_from} ${pmsSnapshotSources().pms_finance_facts_pms_properties_p_join} ON p.id=d.property_id
    LEFT JOIN finance_invoices i ON i.id=d.finance_id AND i.company_id=p.company_id
    LEFT JOIN finance_payments pay ON pay.id=d.finance_id AND pay.company_id=p.company_id
    WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?)
    ORDER BY d.created_at DESC,d.id LIMIT 100`).bind(facts.companyId, facts.branchId, facts.branchId2, facts.propertyId, facts.propertyId2).all<{
            id: string;
            property_id: string;
            folio_id: string;
            kind: string;
            finance_id: string | null;
            journal_id: string | null;
            finance_kind: string;
            created_at: number;
        }>()
    };
}
