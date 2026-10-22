import {calendarPeopleProjectionSources} from "@erpx/server-people_payroll/public/calendar-people-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { type RecurrenceRule } from "@erpx/feature-work_projection/lib/calendar-recurrence";
import type { AppointmentRow } from "@erpx/server-work_projection/domain/calendar/appointments";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarAppointmentsWorkProjectionFacts(db: SqlDatabase) {
    return { read1: (facts: {
            value1: boolean;
            value2: string;
            value3: string | null;
            value4: string;
            value5: string;
            value6: number;
            value7: number;
            value8: boolean;
            value9: string | null;
            value10: "mine" | "team";
            value11: string | null;
            value12: string;
            value13: number;
        }) => {
            return db.prepare(`WITH candidates AS (
 SELECT x.*,p.legal_name organizer_name,
 ((? AND ${inList("coalesce(x.department_id,x.branch_id,x.company_id)")}) OR x.organizer_person_id=? OR EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=?)) full_detail
 FROM calendar_appointments x LEFT JOIN (${calendarPeopleProjectionSources.people}) p ON p.id=x.organizer_person_id AND p.company_id=x.company_id
 WHERE x.company_id=? AND x.starts_at<? AND (x.ends_at>? OR EXISTS(SELECT 1 FROM calendar_series s WHERE s.appointment_id=x.id))
 AND (${inList("coalesce(x.department_id,x.branch_id,x.company_id)")} OR EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=?) OR EXISTS(SELECT 1 FROM calendar_appointment_participants ap JOIN (${calendarPeopleProjectionSources.engagements}) e ON e.person_id=ap.person_id AND e.company_id=ap.company_id WHERE ap.appointment_id=x.id AND e.status='active' AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")})))
 SELECT x.* FROM candidates x WHERE (full_detail OR (? AND x.status='scheduled'))
 AND ((?::text IS NOT NULL AND EXISTS(SELECT 1 FROM calendar_appointment_participants ap WHERE ap.appointment_id=x.id AND ap.person_id=?))
   OR (?::text IS NULL AND (?='team' OR EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=?) OR x.organizer_person_id=?)))
 AND (?='' OR (CASE WHEN full_detail THEN x.title ELSE 'Busy' END) ILIKE '%'||?||'%') ORDER BY x.id LIMIT ?`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value2, facts.value4, facts.value2, facts.value8, facts.value9, facts.value9, facts.value9, facts.value10, facts.value4, facts.value11, facts.value12, facts.value12, facts.value13).all<AppointmentRow & {
                full_detail: boolean;
                organizer_name: string | null;
            }>().then(result => result.results);
        },
        read2: (facts: {
            value1: string;
            value2: string;
        }) => {
            return db.prepare(`SELECT ap.appointment_id,ap.user_id,ap.person_id,p.legal_name name,EXISTS(SELECT 1 FROM (${calendarPeopleProjectionSources.engagements}) e WHERE e.person_id=p.id AND e.company_id=p.company_id AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")}) in_scope,ap.attendance,ap.response
          FROM calendar_attendees ap LEFT JOIN (${calendarPeopleProjectionSources.people}) p ON p.id=ap.person_id AND p.company_id=ap.company_id WHERE ${inList("ap.appointment_id")} ORDER BY p.legal_name,ap.user_id`).bind(facts.value1, facts.value2).all<{
                appointment_id: string;
                user_id: string;
                person_id: string | null;
                name: string | null;
                in_scope: boolean;
                attendance: "organizer" | "required" | "optional";
                response: "pending" | "accepted" | "tentative" | "declined";
            }>().then(result => result.results);
        },
        read3: (facts: {
            value1: string;
            value2: string;
        }) => { return db.prepare(`SELECT * FROM calendar_series WHERE company_id=? AND ${inList("appointment_id")}`).bind(facts.value1, facts.value2).all<{
            appointment_id: string;
            frequency: RecurrenceRule["frequency"];
            interval_count: number;
            weekdays: string;
            end_kind: RecurrenceRule["endKind"];
            occurrence_count: number | null;
            until_date: string | null;
        }>(); },
        read4: (facts: {
            value1: string;
            value2: string;
        }) => { return db.prepare(`SELECT * FROM calendar_occurrence_exceptions WHERE company_id=? AND ${inList("appointment_id")}`).bind(facts.value1, facts.value2).all<{
            appointment_id: string;
            occurrence_key: string;
            start_local: string | null;
            end_local: string | null;
            cancelled: boolean;
        }>(); },
        read5: (facts: {
            value1: string;
            value2: string;
        }) => { return db.prepare(`SELECT appointment_id,occurrence_key,user_id,response FROM calendar_occurrence_responses WHERE company_id=? AND ${inList("appointment_id")}`).bind(facts.value1, facts.value2).all<{
            appointment_id: string;
            occurrence_key: string;
            user_id: string;
            response: "accepted" | "tentative" | "declined";
        }>(); },
        read6: (facts: {
            value1: string;
            value2: string;
            value3: number;
            value4: number;
            value5: string;
        }) => { return db.prepare(`SELECT 1 FROM calendar_appointments a JOIN calendar_appointment_participants p ON p.appointment_id=a.id WHERE a.company_id=? AND a.status='scheduled' AND a.id!=? AND a.starts_at<? AND a.ends_at>? AND ${inList("p.person_id")} LIMIT 1`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5).first<Record<string, unknown>>(); },
        read8: (facts: {
            value1: string;
            value2: string;
            value3: number;
            value4: number;
            value5: string;
        }) => {
            return db.prepare(`SELECT 1 FROM calendar_appointments a JOIN calendar_attendees p ON p.appointment_id=a.id
    WHERE a.company_id=? AND a.status='scheduled' AND a.id!=? AND a.starts_at<? AND a.ends_at>?
    AND p.response!='declined' AND ${inList("p.user_id")} LIMIT 1`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5).first<Record<string, unknown>>();
        } };
}
