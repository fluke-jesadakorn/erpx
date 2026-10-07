import {calendarPeopleProjectionSources} from "../../people_payroll/public/calendar-people-projection-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { AnnouncementRow } from "../../work_projection/domain/calendar/announcements";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarAnnouncementsWorkProjectionFacts(db: SqlDatabase) {
    return { read1: (facts: {
            value1: string;
            value2: string;
            value3: boolean;
            value4: string;
            value5: string;
            value6: string;
            value7: string;
            value8: number;
        }) => {
            return db.prepare(`SELECT n.*,coalesce(p.legal_name,'Publisher') owner_name FROM calendar_announcements n LEFT JOIN (${calendarPeopleProjectionSources.people}) p ON p.user_id=n.created_by AND p.company_id=n.company_id AND p.merged_into_id IS NULL
 WHERE n.company_id=? AND ${inList("coalesce(n.department_id,n.branch_id,n.company_id)")} AND (n.status='published' OR (? AND ${inList("coalesce(n.department_id,n.branch_id,n.company_id)")}))
 AND n.start_date<? AND n.end_date>=? AND (?='' OR n.title ILIKE '%'||?||'%' OR n.content ILIKE '%'||?||'%') ORDER BY n.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value7, facts.value7, facts.value8).all<AnnouncementRow & {
                owner_name: string;
            }>().then(result => result.results);
        } };
}
