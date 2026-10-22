import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { type RecurrenceRule } from "@erpx/feature-work_projection/lib/calendar-recurrence";
export function calendarAvailabilityWorkProjectionFacts(db: SqlDatabase) {
    return { read2: (facts: {
            value1: string;
            value2: number;
            value3: string[];
        }) => {
            return db.prepare(`SELECT a.id,a.starts_at,a.ends_at,a.timezone,a.status,a.all_day,
    s.frequency,s.interval_count,s.weekdays,s.end_kind,s.occurrence_count,s.until_date,t.user_id,t.response
    FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id
    LEFT JOIN calendar_series s ON s.appointment_id=a.id
    WHERE a.company_id=? AND a.status='scheduled' AND a.starts_at<? AND t.response!='declined'
    AND t.user_id IN (${facts.value3.map(() => "?").join(",")})`).bind(facts.value1, facts.value2, ...facts.value3).all<{
                id: string;
                starts_at: number;
                ends_at: number;
                timezone: string;
                frequency: RecurrenceRule["frequency"] | null;
                interval_count: number | null;
                weekdays: string | null;
                end_kind: RecurrenceRule["endKind"] | null;
                occurrence_count: number | null;
                until_date: string | null;
                user_id: string;
            }>();
        },
        read3: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => {
            return db.prepare("SELECT start_local,end_local,cancelled FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND occurrence_key=?").bind(facts.value1, facts.value2, facts.value3).first<{
                start_local: string | null;
                end_local: string | null;
                cancelled: boolean;
            }>();
        },
        read4: (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: string;
        }) => {
            return db.prepare("SELECT response FROM calendar_occurrence_responses WHERE appointment_id=? AND company_id=? AND occurrence_key=? AND user_id=?").bind(facts.value1, facts.value2, facts.value3, facts.value4).first<{
                response: string;
            }>();
        },
        read5: (facts: {
            value1: string;
            value2: string;
        }) => {
            return db.prepare("SELECT occurrence_key,start_local,end_local FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND cancelled=false AND start_local IS NOT NULL AND end_local IS NOT NULL").bind(facts.value1, facts.value2).all<{
                occurrence_key: string;
                start_local: string;
                end_local: string;
            }>();
        },
        read6: (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: string;
        }) => {
            return db.prepare("SELECT response FROM calendar_occurrence_responses WHERE appointment_id=? AND company_id=? AND occurrence_key=? AND user_id=?").bind(facts.value1, facts.value2, facts.value3, facts.value4).first<{
                response: string;
            }>();
        } };
}
