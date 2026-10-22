import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function calendarAvailabilityPeoplePayrollFacts(db: SqlDatabase) {
    return { read7: (facts: {
            value1: string;
            value2: string[];
            value3: string;
            value4: string;
        }) => {
            return db.prepare(`SELECT DISTINCT p.user_id,d.leave_date::text leave_date
    FROM leave_requests l JOIN hr_people p ON p.id=l.person_id AND p.company_id=l.company_id
    CROSS JOIN LATERAL (
      SELECT s.leave_date::date leave_date FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id
      UNION ALL
      SELECT generate_series(l.start_date::date,coalesce(l.end_date::date,l.start_date::date+l.days::int-1),interval '1 day')::date
      WHERE NOT EXISTS(SELECT 1 FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id)
    ) d
    WHERE l.company_id=? AND l.state='approved' AND p.user_id IN (${facts.value2.map(() => "?").join(",")})
    AND d.leave_date BETWEEN ?::date AND ?::date`).bind(facts.value1, ...facts.value2, facts.value3, facts.value4).all<{
                user_id: string;
                leave_date: string;
            }>();
        } };
}
