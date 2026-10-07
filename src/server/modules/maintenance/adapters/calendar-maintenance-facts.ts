import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "../../people_payroll/public/final-root-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarMaintenanceMaintenanceFacts(db: SqlDatabase) {
    return { read1: (facts: {
            value1: string;
            value2: string | null;
            value3: string | null;
            value4: string;
            value5: number;
            value6: number;
            value7: "mine" | "team";
            value8: string;
            value9: string | null;
            value10: string;
            value11: number;
        }) => {
            return db.prepare(`WITH scheduled AS (
  SELECT p.id,'maintenance_plan' source_kind,p.company_id,p.branch_id,p.department_id,p.title,p.status,p.version,p.next_due_at due_at,p.owner_id,p.created_by
  FROM maintenance_preventive_plans p WHERE p.status='active'
  UNION ALL SELECT w.id,'maintenance_work',w.company_id,w.branch_id,w.department_id,w.title,w.status,w.version,w.scheduled_at,w.owner_id,w.created_by
  FROM maintenance_work_orders w WHERE w.status NOT IN ('completed','cancelled') AND w.scheduled_at IS NOT NULL)
  SELECT s.*,(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=s.company_id AND user_id=s.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name
  FROM scheduled s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?::text IS NULL OR s.department_id=?) AND ${inList('coalesce(s.department_id,s.branch_id)')}
  AND s.due_at>=? AND s.due_at<? AND (?='team' OR s.owner_id=? OR s.created_by=?)
  AND (?::text IS NULL OR s.owner_id IN (SELECT user_id FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE id=? AND company_id=s.company_id))
  AND (?='' OR s.title ILIKE '%'||?||'%') ORDER BY s.due_at,s.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value2, facts.value3, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8, facts.value8, facts.value9, facts.value9, facts.value10, facts.value10, facts.value11).all<{
                id: string;
                source_kind: string;
                title: string;
                branch_id: string;
                department_id: string | null;
                status: string;
                version: number;
                due_at: number;
                owner_name: string | null;
            }>().then(result => result.results);
        } };
}
