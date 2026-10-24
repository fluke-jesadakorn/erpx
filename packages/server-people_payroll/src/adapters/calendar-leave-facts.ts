import {workforcePurchasedHistorySql} from "@erpx/server-people_payroll/adapters/workforce-purchases";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
import {participantSql} from '@erpx/server-workflow/public/approval-participant-projection';
export function calendarLeavePeoplePayrollFacts(db: SqlDatabase) {
    return { read1: async(facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: string;
            value5: string;
            value6: string;
            value7: boolean;
            value8: string | null;
            value9: string;
            value10: number;
        }) => {
            return db.prepare(`WITH visible AS (SELECT l.id,coalesce(l.person_id,e.person_id) person_id,p.legal_name name,l.start_date,l.end_date,l.days,l.state,l.version,coalesce(nullif(l.branch_id,''),nullif(e.branch_id,'')) branch_id,coalesce(nullif(l.department_id,''),nullif(e.department_id,'')) department_id,s.leave_date,s.segment,coalesce(l.calculation_snapshot->'policy'->>'timezone',v.timezone,'UTC') timezone,
 (l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l", "?", "?")}) full_detail
 FROM leave_requests l LEFT JOIN employees e ON e.id=l.employee_id AND e.company_id=l.company_id JOIN hr_people p ON p.id=coalesce(l.person_id,e.person_id) AND p.company_id=l.company_id
 LEFT JOIN leave_day_segments s ON s.request_id=l.id AND s.company_id=l.company_id LEFT JOIN leave_policy_versions v ON v.id=l.policy_version_id
 WHERE l.company_id=? AND ${await workforcePurchasedHistorySql("l.company_id")} AND coalesce(s.leave_date,l.start_date)<? AND coalesce(s.leave_date::date,l.end_date::date,l.start_date::date+(l.days::int-1))>=?::date)
 SELECT * FROM visible l WHERE ${inList("coalesce(l.department_id,l.branch_id,?)")} AND (full_detail OR (? AND state='approved')) AND state!='draft'
 AND (?::text IS NULL OR person_id=?) AND (?='' OR name ILIKE '%'||?||'%' OR 'Absence' ILIKE '%'||?||'%') ORDER BY id,leave_date LIMIT ?`).bind(facts.value1, facts.value1, facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value3, facts.value6, facts.value7, facts.value8, facts.value8, facts.value9, facts.value9, facts.value9, facts.value10).all<{
                id: string;
                person_id: string;
                name: string;
                start_date: string;
                end_date: string | null;
                days: number;
                state: string;
                version: number;
                branch_id: string | null;
                department_id: string | null;
                full_detail: boolean;
                leave_date: string | null;
                segment: string | null;
                timezone: string;
            }>().then(result => result.results);
        },
        read2: async(facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: string;
            value5: string | null;
            value6: boolean;
            value7: string;
            value8: string;
            value9: number;
        }) => {
            return db.prepare(`SELECT h.id,h.name,h.holiday_date,c.timezone,c.name calendar,e.department_id,e.branch_id,jsonb_agg(DISTINCT jsonb_build_object('id',e.person_id,'name',p.legal_name)) participants
     FROM leave_holidays h JOIN leave_holiday_calendars c ON c.id=h.calendar_id
     JOIN leave_policy_assignments pa ON pa.holiday_calendar_id=c.id AND pa.company_id=c.company_id AND pa.effective_from<=h.holiday_date AND coalesce(pa.effective_to,'9999-12-31')>=h.holiday_date
     JOIN hr_engagements e ON e.id=pa.engagement_id AND e.company_id=pa.company_id JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
     WHERE c.company_id=? AND ${await workforcePurchasedHistorySql("c.company_id")} AND h.holiday_date>=? AND h.holiday_date<? AND e.start_date<=h.holiday_date AND (e.end_date IS NULL OR e.end_date>=h.holiday_date) AND p.merged_into_id IS NULL AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")}
     AND (?::text IS NULL OR e.person_id=?) AND (? OR e.person_id=?) AND (?='' OR h.name ILIKE '%'||?||'%' OR c.name ILIKE '%'||?||'%')
     GROUP BY h.id,h.name,h.holiday_date,c.timezone,c.name,e.department_id,e.branch_id ORDER BY h.id,e.department_id,e.branch_id LIMIT ?`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value5, facts.value6, facts.value7, facts.value8, facts.value8, facts.value8, facts.value9).all<{
                id: string;
                name: string;
                holiday_date: string;
                timezone: string;
                calendar: string;
                participants: string | {
                    id: string;
                    name: string;
                }[];
                department_id: string | null;
                branch_id: string | null;
            }>().then(result => result.results);
        } };
}
