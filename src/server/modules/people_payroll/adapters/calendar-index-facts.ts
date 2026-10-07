import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarIndexPeoplePayrollFacts(db: SqlDatabase) {
    return { read1: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => { return db.prepare(`SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.id=? AND p.company_id=? AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")} LIMIT 1`).bind(facts.value1, facts.value2, facts.value3).first<Record<string, unknown>>(); },
        read3: (facts: {
            value1: string;
            value2: string;
            value3: number;
            value4: string;
            value5: number;
        }) => {
            return db.prepare(`SELECT 1 FROM leave_requests l LEFT JOIN employees e ON e.id=l.employee_id AND e.company_id=l.company_id WHERE l.company_id=? AND l.state='approved' AND ${inList("coalesce(l.person_id,e.person_id)")} AND EXISTS(SELECT 1 FROM (
 SELECT s.leave_date::date absence_date FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id
 UNION ALL SELECT generate_series(l.start_date::date,coalesce(l.end_date::date,l.start_date::date+l.days::int-1),interval '1 day')::date WHERE NOT EXISTS(SELECT 1 FROM leave_day_segments s WHERE s.request_id=l.id)
 ) absence WHERE absence_date<=(to_timestamp((?::bigint-1)/1000.0) AT TIME ZONE ?)::date AND absence_date>=(to_timestamp(?::bigint/1000.0) AT TIME ZONE ?)::date) LIMIT 1`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value4).first<Record<string, unknown>>();
        } };
}
