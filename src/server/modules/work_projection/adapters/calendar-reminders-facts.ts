import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";

import type {Row} from "../../work_projection/public/calendar/reminders";
import {workPackageEnforcementMode} from '../../commercial/public/purchase-command-boundary';
import {calendarReminderCandidateScope} from './calendar/reminder-authority';
export function calendarRemindersWorkProjectionFacts(db:SqlDatabase){return {read1:async(facts:{value1:number;value2:number;value3:string;})=>db.prepare(`SELECT a.id,a.company_id,a.title,a.version,a.starts_at,a.ends_at,a.timezone,
      s.frequency,s.interval_count,s.weekdays,s.end_kind,s.occurrence_count,s.until_date,
      t.user_id,t.reminder_minutes,t.response FROM calendar_appointments a
      JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id
      LEFT JOIN calendar_series s ON s.appointment_id=a.id
      WHERE a.status='scheduled' AND a.starts_at<? AND t.reminder_minutes IS NOT NULL AND t.response!='declined'
      AND (s.appointment_id IS NOT NULL OR a.ends_at>?) AND (a.id||':'||t.user_id)>?
      AND ${await workPackageEnforcementMode()==='enforced'?calendarReminderCandidateScope():'TRUE'}
      ORDER BY a.id,t.user_id LIMIT 200`).bind(facts.value1,facts.value2,facts.value3).all<Row>(),
read2:(facts:{value1:string;value2:string;})=>db.prepare(`SELECT occurrence_key,start_local,end_local FROM calendar_occurrence_exceptions
          WHERE appointment_id=? AND company_id=? AND cancelled=false AND start_local IS NOT NULL AND end_local IS NOT NULL`).bind(facts.value1,facts.value2).all<{occurrence_key:string;start_local:string;end_local:string}>(),
read3:(facts:{value1:string;value2:string;value3:string;})=>db.prepare("SELECT start_local,end_local,cancelled FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND occurrence_key=?").bind(facts.value1,facts.value2,facts.value3).first<{start_local:string|null;end_local:string|null;cancelled:boolean}>(),
read4:(facts:{value1:string;value2:string;value3:string;value4:string;})=>db.prepare("SELECT response FROM calendar_occurrence_responses WHERE appointment_id=? AND company_id=? AND occurrence_key=? AND user_id=?").bind(facts.value1,facts.value2,facts.value3,facts.value4).first<{response:string}>(),
write5:(facts:{value1:string;value2:string;value3:string;value4:string;value5:number;})=>db.prepare(`INSERT INTO calendar_reminder_deliveries(company_id,appointment_id,occurrence_key,user_id,due_at)
            VALUES(?,?,?,?,?) ON CONFLICT(appointment_id,occurrence_key,user_id) DO UPDATE SET
            due_at=excluded.due_at,delivered_at=CASE WHEN calendar_reminder_deliveries.due_at=excluded.due_at
              THEN calendar_reminder_deliveries.delivered_at ELSE NULL END`).bind(facts.value1,facts.value2,facts.value3,facts.value4,facts.value5).run(),
read6:(facts:{value1:number;value2:string;value3:string;value4:string;value5:number;})=>db.prepare(`UPDATE calendar_reminder_deliveries SET delivered_at=?
            WHERE appointment_id=? AND occurrence_key=? AND user_id=? AND due_at=? AND delivered_at IS NULL
            RETURNING user_id`).bind(facts.value1,facts.value2,facts.value3,facts.value4,facts.value5).first()};}
