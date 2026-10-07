import { financeSnapshotSources as people_payrollSnapshot } from "../../people_payroll/public/finance-snapshot-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function calendarSupplierBatchesFinanceFacts(db: SqlDatabase) {
    return { read1: (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: "mine" | "team";
            value5: string;
            value6: string | null;
            value7: string;
            value8: number;
        }) => {
            return db.prepare(`
 SELECT b.id,b.planned_date,b.status,b.version,(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status='unknown') unknown_count
 FROM finance_supplier_batches b WHERE b.company_id=? AND b.planned_date>=? AND b.planned_date<? AND b.status<>'cancelled'
 AND (?='team' OR b.created_by=? OR b.approved_by=? OR b.exported_by=?)
 AND (?::text IS NULL OR ${people_payrollSnapshot().batchPerson})
 AND (?='' OR b.id ILIKE '%'||?||'%') ORDER BY b.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value5, facts.value5, facts.value6, facts.value6, facts.value7, facts.value7, facts.value8).all<{
                id: string;
                planned_date: string;
                status: string;
                version: number;
                unknown_count: number;
            }>().then(result => result.results);
        } };
}
