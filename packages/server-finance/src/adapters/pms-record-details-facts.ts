import {financeOwnedSnapshotSources as pmsSnapshotSources} from "@erpx/server-pms/public/finance-owned-snapshot-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
const joins = `FROM finance_journal_lines l JOIN (SELECT property_id,journal_id ${pmsSnapshotSources().pms_record_details_facts_pms_postings_from} UNION SELECT property_id,opening_journal_id journal_id ${pmsSnapshotSources().pms_record_details_facts_pms_import_batches_from} WHERE state='applied' AND opening_journal_id IS NOT NULL) p ON p.journal_id=l.journal_id JOIN finance_journals j ON j.id=l.journal_id WHERE p.property_id=? AND j.status='posted' AND l.account_id=?`;
export function pmsRecordDetailFacts(db: SqlDatabase) {
    return {
        controlRows: (facts: {
            propertyId: string;
            account: string;
            offset: number;
        }) => db.prepare(`SELECT l.*,j.id journal_id,j.journal_number,j.entry_date,j.status,j.currency ${joins} ORDER BY j.entry_date,j.id,l.ordinal LIMIT 31 OFFSET ?`).bind(facts.propertyId, facts.account, facts.offset).all(),
        controlBalance: (facts: {
            propertyId: string;
            account: string;
        }) => db.prepare(`SELECT COALESCE(SUM(l.debit_minor-l.credit_minor),0) balance ${joins}`).bind(facts.propertyId, facts.account).first<{
            balance: number;
        }>()
    };
}
