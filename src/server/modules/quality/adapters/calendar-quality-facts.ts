import { commercialOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "../../people_payroll/public/commercial-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarQualityQualityFacts(db: SqlDatabase) { return { read1: (facts: {
        value1: string;
        value2: string | null;
        value3: string;
        value4: string;
        value5: string;
        value6: "mine" | "team";
        value7: string;
        value8: string | null;
        value9: string;
        value10: number;
    }) => { return db.prepare(`SELECT h.id,h.due_date,h.status,h.version,h.branch_id,p.name product_name FROM inventory_quality_holds h JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=h.product_id WHERE h.company_id=? AND (?::text IS NULL OR h.branch_id=?) AND ${inList('h.branch_id')} AND h.due_date>=? AND h.due_date<? AND h.status NOT IN ('released','cancelled','disposed') AND (?='team' OR h.created_by=? OR h.last_inspector=?) AND (?::text IS NULL OR h.created_by IN (SELECT user_id FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE id=? AND company_id=h.company_id)) AND (?='' OR p.name ILIKE '%'||?||'%') ORDER BY h.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value7, facts.value8, facts.value8, facts.value9, facts.value9, facts.value10).all<{
        id: string;
        due_date: string;
        status: string;
        version: number;
        branch_id: string;
        product_name: string;
    }>().then(result => result.results); } }; }
