import type {WorkContext} from "../../identity_access/public/authorization";
import {financeBankCalendarPurchase} from "./calendar-purchases";
import { financeSnapshotSources as commerceSnapshot } from "../../commerce/public/finance-snapshot-sources";
import { financeSnapshotSources as people_payrollSnapshot } from "../../people_payroll/public/finance-snapshot-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarRefundsFinanceFacts(db: SqlDatabase,actor:WorkContext) {
    return { read1: async (facts: {
            value1: string;
            value2: string | null;
            value3: string | null;
            value4: string;
            value5: string;
            value6: string;
            value7: "mine" | "team";
            value8: string;
            value9: string | null;
            value10: string;
            value11: number;
        }) => {
            const purchase=await financeBankCalendarPurchase(db,actor,"o");
            return db.prepare(purchase.clock+`
 SELECT i.id,i.obligation_id,i.planned_date,i.status,i.version,i.prepared_by,o.branch_id,o.department_id,COALESCE(r.number,c.number) number,${people_payrollSnapshot().refundOwner}
 FROM finance_refund_instructions i JOIN finance_refund_obligations o ON o.id=i.obligation_id ${commerceSnapshot().optionalRefundReturn} LEFT JOIN finance_billing_corrections c ON c.id=o.billing_correction_id AND c.company_id=o.company_id
 WHERE (r.id IS NOT NULL OR c.id IS NOT NULL) AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)
 AND ${inList('coalesce(o.department_id,o.branch_id,o.company_id)')}
 AND i.planned_date>=? AND i.planned_date<? AND i.status NOT IN ('cancelled','failed')
 AND (?='team' OR i.prepared_by=? OR o.created_by=? OR o.approved_by=?)
 AND (?::text IS NULL OR ${people_payrollSnapshot().refundPerson})
 AND (?='' OR COALESCE(r.number,c.number) ILIKE '%'||?||'%') AND ${purchase.where} ORDER BY i.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value2, facts.value3, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8, facts.value8, facts.value8, facts.value9, facts.value9, facts.value10, facts.value10, ...purchase.args, facts.value11).all<{
                id: string;
                obligation_id: string;
                planned_date: string;
                status: string;
                version: number;
                branch_id: string;
                department_id: string | null;
                prepared_by: string;
                owner_name: string;
                number: string;
            }>().then(result => result.results);
        } };
}
