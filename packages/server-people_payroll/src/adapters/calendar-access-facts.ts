import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { CalendarPerson } from "@erpx/feature-work_projection/lib/calendar";
const idList = (ids: string[]) => JSON.stringify(ids), inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarAccessPeoplePayrollFacts(db: SqlDatabase) {
    return { people: (input: {
            companyId: string;
            peopleIds: string[];
            lifecycleReader: boolean;
            search: string;
            take: number;
            cursor: number;
        }) => db.prepare(`SELECT * FROM (SELECT DISTINCT ON(p.id) p.id,p.legal_name name,p.user_id "userId",e.department_id "departmentId",e.branch_id "branchId",e.manager_person_id "managerId",e.status
  FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
  WHERE p.company_id=? AND p.merged_into_id IS NULL AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")}
  AND ((? AND (e.status!='ended' OR EXISTS(SELECT 1 FROM people_lifecycle_cases lc WHERE lc.engagement_id=e.id AND lc.company_id=e.company_id))) OR (e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))) AND (?='' OR p.legal_name ILIKE '%'||?||'%')
  ORDER BY p.id,CASE WHEN e.status='active' THEN 0 ELSE 1 END,e.start_date DESC,e.id) x ORDER BY name,id LIMIT ? OFFSET ?`).bind(input.companyId, idList(input.peopleIds), input.lifecycleReader, input.search, input.search, input.take, input.cursor).all<CalendarPerson>().then(result => result.results) };
}
