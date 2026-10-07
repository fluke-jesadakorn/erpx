import { commercialOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "../../people_payroll/public/commercial-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarCustomerFollowupsSalesFacts(db: SqlDatabase) { return { read1: (facts: {
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
    }) => { return db.prepare(`SELECT c.*,o.branch_id,o.department_id,(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=c.company_id AND user_id=c.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name FROM customer_followups c JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE c.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND ${inList('coalesce(o.department_id,o.branch_id,o.company_id)')} AND c.status='open' AND c.due_date>=? AND c.due_date<? AND (?='team' OR c.owner_id=? OR c.created_by=?) AND (?::text IS NULL OR c.owner_id IN(SELECT user_id FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE id=? AND company_id=c.company_id)) AND (?='' OR c.next_action ILIKE '%'||?||'%') ORDER BY c.due_date,c.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value2, facts.value3, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8, facts.value8, facts.value9, facts.value9, facts.value10, facts.value10, facts.value11).all<{
        id: string;
        next_action: string;
        due_date: string;
        status: string;
        version: number;
        branch_id: string;
        department_id: string | null;
        owner_id: string;
        owner_name: string | null;
    }>().then(result => result.results); } }; }
