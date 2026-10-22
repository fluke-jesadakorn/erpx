import {financeOwnedSnapshotSources as pmsSnapshotSources} from "@erpx/server-pms/public/finance-owned-snapshot-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function pmsReportingBalances(db: SqlDatabase) {
    return {
        balances: (facts: {
            propertyId: string;
            to: string;
        }) => db.prepare(`SELECT l.account_id,SUM(l.debit_minor-l.credit_minor) balance FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
 JOIN (SELECT property_id,journal_id ${pmsSnapshotSources().pms_reporting_facts_pms_postings_from} UNION SELECT property_id,opening_journal_id journal_id ${pmsSnapshotSources().pms_reporting_facts_pms_import_batches_from} WHERE state='applied' AND opening_journal_id IS NOT NULL) p ON p.journal_id=j.id
 WHERE p.property_id=? AND j.status='posted' AND j.entry_date<=? GROUP BY l.account_id`).bind(facts.propertyId, facts.to).all<{
            account_id: string;
            balance: number;
        }>()
    };
}
