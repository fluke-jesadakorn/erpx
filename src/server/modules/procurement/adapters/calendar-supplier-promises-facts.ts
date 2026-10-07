import { platformOwnerProjectionSources as commercePlatformSources } from "../../commerce/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "../../people_payroll/public/platform-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarSupplierPromisesProcurementFacts(db: SqlDatabase) { return { read1: (facts: {
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
    }) => { return db.prepare(`SELECT p.id,h.order_id,o.number,p.promised_date,p.remaining_milli,p.description,o.version,o.branch_id,o.department_id FROM procurement_open_promise_lines p JOIN procurement_delivery_promises h ON h.id=p.promise_id JOIN (${commercePlatformSources.commerce_orders}) o ON o.id=h.order_id WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND ${inList('coalesce(o.department_id,o.branch_id,o.company_id)')} AND p.remaining_milli>0 AND p.promised_date>=? AND p.promised_date<? AND (?='team' OR h.recorded_by=? OR o.created_by=?) AND (?::text IS NULL OR h.recorded_by IN(SELECT user_id FROM (${people_payrollPlatformSources.hr_people}) hr_people WHERE id=? AND company_id=o.company_id)) AND (?='' OR o.number ILIKE '%'||?||'%' OR p.description ILIKE '%'||?||'%') ORDER BY p.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value2, facts.value3, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8, facts.value8, facts.value9, facts.value9, facts.value10, facts.value10, facts.value10, facts.value11).all<{
        id: string;
        order_id: string;
        number: string;
        promised_date: string;
        remaining_milli: number;
        description: string;
        version: number;
        branch_id: string;
        department_id: string | null;
    }>().then(result => result.results); } }; }
