import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "../../people_payroll/public/commercial-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarCrmCrmFacts(db: SqlDatabase) { return { read1: (facts: {
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
    }) => { return db.prepare(`SELECT l.id,l.title,COALESCE(v.close_date,l.due_date) due_date,COALESCE(v.stage,l.status) status,l.version,l.branch_id,l.department_id,(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=l.company_id AND user_id=l.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name FROM crm_leads l LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?) AND ${inList('coalesce(l.department_id,l.branch_id,l.company_id)')} AND l.status<>'disqualified' AND (v.stage IS NULL OR v.stage NOT IN ('won','lost')) AND COALESCE(v.close_date,l.due_date)>=? AND COALESCE(v.close_date,l.due_date)<? AND (?='team' OR l.owner_id=? OR l.created_by=?) AND (?::text IS NULL OR l.owner_id IN(SELECT user_id FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE id=? AND company_id=l.company_id)) AND (?='' OR l.title ILIKE '%'||?||'%') ORDER BY due_date,l.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value2, facts.value3, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8, facts.value8, facts.value9, facts.value9, facts.value10, facts.value10, facts.value11).all<{
        id: string;
        title: string;
        due_date: string;
        status: string;
        version: number;
        branch_id: string;
        department_id: string | null;
        owner_name: string | null;
    }>().then(result => result.results); } }; }
