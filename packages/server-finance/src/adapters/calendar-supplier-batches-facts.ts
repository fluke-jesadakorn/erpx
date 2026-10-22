import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {financeBankCalendarPurchase} from "@erpx/server-finance/adapters/calendar-purchases";
import { financeSnapshotSources as people_payrollSnapshot } from "@erpx/server-people_payroll/public/finance-snapshot-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function calendarSupplierBatchesFinanceFacts(db: SqlDatabase,actor:WorkContext) {
    return { read1: async (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: "mine" | "team";
            value5: string;
            value6: string | null;
            value7: string;
            value8: number;
        }) => {
            const purchase=await financeBankCalendarPurchase(db,actor,"b");
            return db.prepare(purchase.clock+`
 SELECT b.id,b.planned_date,b.status,b.version,(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status='unknown') unknown_count
 FROM finance_supplier_batches b WHERE b.company_id=? AND b.planned_date>=? AND b.planned_date<? AND b.status<>'cancelled'
 AND (?='team' OR b.created_by=? OR b.approved_by=? OR b.exported_by=?)
 AND (?::text IS NULL OR ${people_payrollSnapshot().batchPerson})
 AND (?='' OR b.id ILIKE '%'||?||'%') AND ${purchase.where} ORDER BY b.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value5, facts.value5, facts.value6, facts.value6, facts.value7, facts.value7, ...purchase.args, facts.value8).all<{
                id: string;
                planned_date: string;
                status: string;
                version: number;
                unknown_count: number;
            }>().then(result => result.results);
        } };
}
