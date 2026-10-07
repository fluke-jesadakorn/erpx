import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { SeriesRow } from "../../work_projection/public/calendar/commands";
import type { AppointmentRow } from "../../work_projection/public/calendar/appointments";
import type { AnnouncementRow } from "../../work_projection/public/calendar/announcements";
export function calendarCommandsWorkProjectionFacts(db: SqlDatabase) {
    return { read1: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => db.prepare(`SELECT (extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint starts_at,(extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint ends_at,to_char((?::timestamp AT TIME ZONE ?) AT TIME ZONE ?,'YYYY-MM-DD"T"HH24:MI') start_local,to_char((?::timestamp AT TIME ZONE ?) AT TIME ZONE ?,'YYYY-MM-DD"T"HH24:MI') end_local`).bind(facts.value1, facts.value2, facts.value3, facts.value2, facts.value1, facts.value2, facts.value2, facts.value3, facts.value2, facts.value2).first<{
            starts_at: number;
            ends_at: number;
            start_local: string;
            end_local: string;
        }>(),
        read3: (facts: {
            value1: string;
        }) => db.prepare("SELECT frequency,interval_count,weekdays,end_kind,occurrence_count,until_date FROM calendar_series WHERE appointment_id=?").bind(facts.value1).first<SeriesRow>(),
        read4: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("SELECT user_id FROM calendar_attendees WHERE appointment_id=? AND company_id=?").bind(facts.value1, facts.value2).all<{
            user_id: string;
        }>(),
        read12: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("SELECT * FROM calendar_appointments WHERE id=? AND company_id=?").bind(facts.value1, facts.value2).first<AppointmentRow>(),
        read13: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("SELECT person_id FROM calendar_appointment_participants WHERE appointment_id=? AND company_id=?").bind(facts.value1, facts.value2).all<{
            person_id: string;
        }>().then(result => result.results),
        write14: (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: string;
            value5: string;
        }) => db.prepare(`INSERT INTO calendar_occurrence_exceptions(company_id,appointment_id,occurrence_key,start_local,end_local)
          VALUES(?,?,?,?,?) ON CONFLICT(appointment_id,occurrence_key) DO UPDATE SET start_local=excluded.start_local,end_local=excluded.end_local,cancelled=false,version=calendar_occurrence_exceptions.version+1`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5).run(),
        write15: (facts: {
            value1: number;
            value2: string;
            value3: string;
        }) => db.prepare("UPDATE calendar_appointments SET version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(facts.value1, facts.value2, facts.value3).run(),
        write16: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("UPDATE calendar_series SET end_kind='until',until_date=?,occurrence_count=NULL,version=version+1 WHERE appointment_id=?").bind(facts.value1, facts.value2).run(),
        write18: (facts: {
            value1: string;
            value2: number;
            value3: number;
            value4: string;
            value5: number;
            value6: boolean;
            value7: string | null;
            value8: string | null;
            value9: string;
            value10: string;
        }) => db.prepare(`INSERT INTO calendar_appointments(id,company_id,branch_id,department_id,organizer_person_id,title,description,starts_at,ends_at,timezone,location,meeting_url,private,guests,status,version,created_by,created_at,updated_at,all_day,start_date,end_date)
          SELECT ?,company_id,branch_id,department_id,organizer_person_id,title,description,?,?,?,location,meeting_url,private,guests,status,1,created_by,?,?,?,?,?
          FROM calendar_appointments WHERE id=? AND company_id=?`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value5, facts.value6, facts.value7, facts.value8, facts.value9, facts.value10).run(),
        write19: (facts: {
            value1: string;
            value2: string;
            value3: "daily" | "weekly" | "monthly" | "yearly";
            value4: number;
            value5: string;
            value6: "never" | "count" | "until";
            value7: number | null;
            value8: string | null;
        }) => db.prepare(`INSERT INTO calendar_series(appointment_id,company_id,frequency,interval_count,weekdays,end_kind,occurrence_count,until_date)
          VALUES(?,?,?,?,?,?,?,?)`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8).run(),
        write20: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => db.prepare(`INSERT INTO calendar_attendees(company_id,appointment_id,user_id,person_id,attendance,response,reminder_minutes)
          SELECT company_id,?,user_id,person_id,attendance,CASE WHEN attendance='organizer' THEN 'accepted' ELSE 'pending' END,reminder_minutes
          FROM calendar_attendees WHERE appointment_id=? AND company_id=?`).bind(facts.value1, facts.value2, facts.value3).run(),
        write21: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => db.prepare(`INSERT INTO calendar_appointment_participants(company_id,appointment_id,person_id)
          SELECT company_id,?,person_id FROM calendar_appointment_participants WHERE appointment_id=? AND company_id=?`).bind(facts.value1, facts.value2, facts.value3).run(),
        write22: (facts: {
            value1: number;
            value2: number;
            value3: string;
            value4: string | null;
            value5: string | null;
            value6: number;
            value7: string;
            value8: string;
        }) => db.prepare("UPDATE calendar_appointments SET starts_at=?,ends_at=?,timezone=?,start_date=?,end_date=?,version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8).run(),
        read23: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("SELECT * FROM calendar_announcements WHERE id=? AND company_id=?").bind(facts.value1, facts.value2).first<AnnouncementRow>(),
        write24: (facts: {
            value1: string;
            value2: string;
            value3: number;
            value4: string;
            value5: string;
        }) => db.prepare("UPDATE calendar_announcements SET start_date=?,end_date=?,version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5).run(),
        read26: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("SELECT * FROM calendar_appointments WHERE id=? AND company_id=?").bind(facts.value1, facts.value2).first<AppointmentRow>(),
        write29: (facts: {
            value1: string;
            value2: string;
            value3: string | null;
            value4: string | null;
            value5: string | null;
            value6: string;
            value7: string;
            value8: number;
            value9: number;
            value10: string;
            value11: string;
            value12: string;
            value13: boolean;
            value14: string;
            value15: number;
            value16: string;
            value17: number;
            value18: number;
            value19: boolean;
            value20: string | null;
            value21: string | null;
        }) => db.prepare(`INSERT INTO calendar_appointments(id,company_id,branch_id,department_id,organizer_person_id,title,description,starts_at,ends_at,timezone,location,meeting_url,private,guests,status,version,created_by,created_at,updated_at,all_day,start_date,end_date) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?::jsonb,'scheduled',?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET branch_id=excluded.branch_id,department_id=excluded.department_id,organizer_person_id=excluded.organizer_person_id,title=excluded.title,description=excluded.description,starts_at=excluded.starts_at,ends_at=excluded.ends_at,timezone=excluded.timezone,location=excluded.location,meeting_url=excluded.meeting_url,private=excluded.private,guests=excluded.guests,version=excluded.version,updated_at=excluded.updated_at,all_day=excluded.all_day,start_date=excluded.start_date,end_date=excluded.end_date`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8, facts.value9, facts.value10, facts.value11, facts.value12, facts.value13, facts.value14, facts.value15, facts.value16, facts.value17, facts.value18, facts.value19, facts.value20, facts.value21).run(),
        read30: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("SELECT user_id,response,reminder_minutes,response_at FROM calendar_attendees WHERE appointment_id=? AND company_id=?").bind(facts.value1, facts.value2).all<{
            user_id: string;
            response: string;
            reminder_minutes: number | null;
            response_at: number | null;
        }>(),
        write31: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("DELETE FROM calendar_attendees WHERE appointment_id=? AND company_id=?").bind(facts.value1, facts.value2).run(),
        write32: (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: string | null;
            value5: "optional" | "organizer" | "required";
            value6: string;
            value7: number | null;
            value8: number | null;
        }) => db.prepare(`INSERT INTO calendar_attendees(company_id,appointment_id,user_id,person_id,attendance,response,reminder_minutes,response_at)
          VALUES(?,?,?,?,?,?,?,?)`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8).run(),
        write33: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("DELETE FROM calendar_appointment_participants WHERE appointment_id=? AND company_id=?").bind(facts.value1, facts.value2).run(),
        write34: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => db.prepare("INSERT INTO calendar_appointment_participants(company_id,appointment_id,person_id) VALUES(?,?,?)").bind(facts.value1, facts.value2, facts.value3).run(),
        write35: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("DELETE FROM calendar_series WHERE appointment_id=? AND company_id=?").bind(facts.value1, facts.value2).run(),
        write36: (facts: {
            value1: string;
            value2: string;
            value3: "daily" | "weekly" | "monthly" | "yearly";
            value4: number;
            value5: string;
            value6: "never" | "count" | "until";
            value7: number | null;
            value8: string | null;
        }) => db.prepare(`INSERT INTO calendar_series(appointment_id,company_id,frequency,interval_count,weekdays,end_kind,occurrence_count,until_date)
          VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(appointment_id) DO UPDATE SET frequency=excluded.frequency,interval_count=excluded.interval_count,
          weekdays=excluded.weekdays,end_kind=excluded.end_kind,occurrence_count=excluded.occurrence_count,until_date=excluded.until_date,version=calendar_series.version+1`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8).run(),
        read37: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("SELECT * FROM calendar_appointments WHERE id=? AND company_id=?").bind(facts.value1, facts.value2).first<AppointmentRow>(),
        read38: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => db.prepare("SELECT attendance FROM calendar_attendees WHERE company_id=? AND appointment_id=? AND user_id=?").bind(facts.value1, facts.value2, facts.value3).first<{
            attendance: string;
        }>(),
        write39: (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: string;
            value5: "accepted" | "tentative" | "declined";
            value6: number;
        }) => db.prepare(`INSERT INTO calendar_occurrence_responses(company_id,appointment_id,occurrence_key,user_id,response,responded_at)
          VALUES(?,?,?,?,?,?) ON CONFLICT(appointment_id,occurrence_key,user_id) DO UPDATE SET response=excluded.response,responded_at=excluded.responded_at`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6).run(),
        write40: (facts: {
            value1: "accepted" | "tentative" | "declined";
            value2: number;
            value3: string;
            value4: string;
            value5: string;
        }) => db.prepare("UPDATE calendar_attendees SET response=?,response_at=? WHERE company_id=? AND appointment_id=? AND user_id=?").bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5).run(),
        read42: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("SELECT user_id FROM calendar_attendees WHERE appointment_id=? AND company_id=? AND attendance='organizer'").bind(facts.value1, facts.value2).first<{
            user_id: string;
        }>(),
        read44: (facts: {
            value1: number | null;
            value2: string;
            value3: string;
            value4: string;
        }) => db.prepare("UPDATE calendar_attendees SET reminder_minutes=? WHERE company_id=? AND appointment_id=? AND user_id=? RETURNING user_id").bind(facts.value1, facts.value2, facts.value3, facts.value4).first(),
        write45: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => db.prepare("DELETE FROM calendar_reminder_deliveries WHERE company_id=? AND appointment_id=? AND user_id=? AND delivered_at IS NULL").bind(facts.value1, facts.value2, facts.value3).run(),
        write48: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => db.prepare(`INSERT INTO calendar_occurrence_exceptions(company_id,appointment_id,occurrence_key,cancelled)
          VALUES(?,?,?,true) ON CONFLICT(appointment_id,occurrence_key) DO UPDATE SET cancelled=true,version=calendar_occurrence_exceptions.version+1`).bind(facts.value1, facts.value2, facts.value3).run(),
        write50: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("UPDATE calendar_series SET end_kind='until',until_date=?,occurrence_count=NULL,version=version+1 WHERE appointment_id=?").bind(facts.value1, facts.value2).run(),
        write52: (facts: {
            value1: "cancelled" | "completed" | "no_show";
            value2: number;
            value3: string;
            value4: string;
        }) => db.prepare("UPDATE calendar_appointments SET status=?,version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(facts.value1, facts.value2, facts.value3, facts.value4).run(),
        read53: (facts: {
            value1: string;
            value2: string;
        }) => db.prepare("SELECT * FROM calendar_announcements WHERE id=? AND company_id=?").bind(facts.value1, facts.value2).first<AnnouncementRow>(),
        write54: (facts: {
            value1: string;
            value2: string;
            value3: string | null;
            value4: string | null;
            value5: string;
            value6: string;
            value7: string;
            value8: string;
            value9: string;
            value10: number;
            value11: string;
            value12: number;
            value13: number;
        }) => db.prepare(`INSERT INTO calendar_announcements(id,company_id,branch_id,department_id,title,content,start_date,end_date,status,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET branch_id=excluded.branch_id,department_id=excluded.department_id,title=excluded.title,content=excluded.content,start_date=excluded.start_date,end_date=excluded.end_date,version=excluded.version,updated_at=excluded.updated_at`).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8, facts.value9, facts.value10, facts.value11, facts.value12, facts.value13).run(),
        write56: (facts: {
            value1: "published" | "withdrawn";
            value2: number;
            value3: string;
            value4: string;
        }) => db.prepare("UPDATE calendar_announcements SET status=?,version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(facts.value1, facts.value2, facts.value3, facts.value4).run(),
        write57: (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: string;
            value5: string;
            value6: string;
            value7: number;
            value8: number;
        }) => db.prepare("INSERT INTO calendar_history(id,company_id,source,record_id,action,actor_id,version,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value6, facts.value7, facts.value8).run() };
}
