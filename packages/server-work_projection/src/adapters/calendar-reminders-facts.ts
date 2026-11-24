import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {Row} from "@erpx/server-work_projection/public/calendar/reminders";
import {appsPackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import {calendarReminderCandidateScope} from '@erpx/server-work_projection/adapters/calendar/reminder-authority';
import {readOccurrenceResponse,readOccurrenceException,readMovedOccurrences} from "./calendar-availability-facts";
import {nativeOwnerWrite,nativeOwnerWriteRow} from "./native-owner-write";
import {noopSql} from "./read-tail-sql";
export function calendarRemindersWorkProjectionFacts(db:SqlDatabase){return {read1:async(facts:{value1:number;value2:number;value3:string;})=>db.prepare(`SELECT a.id,a.company_id,a.title,a.version,a.starts_at,a.ends_at,a.timezone,
      s.frequency,s.interval_count,s.weekdays,s.end_kind,s.occurrence_count,s.until_date,
      t.user_id,t.reminder_minutes,t.response FROM calendar_appointments a
      JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id
      LEFT JOIN calendar_series s ON s.appointment_id=a.id
      WHERE a.status='scheduled' AND a.starts_at<? AND t.reminder_minutes IS NOT NULL AND t.response!='declined'
      AND (s.appointment_id IS NOT NULL OR a.ends_at>?) AND (a.id||':'||t.user_id)>?
      AND ${await appsPackageEnforcementMode()==='enforced'?calendarReminderCandidateScope():'TRUE'}
      ORDER BY a.id,t.user_id LIMIT 200`).bind(facts.value1,facts.value2,facts.value3).all<Row>(),
read2:(facts:{value1:string;value2:string;})=>readMovedOccurrences(db,facts),
read3:(facts:{value1:string;value2:string;value3:string;})=>readOccurrenceException(db,facts),
read4:(facts:{value1:string;value2:string;value3:string;value4:string;})=>readOccurrenceResponse(db,facts),
// Both writes execute in the native Work axum command. The inert SELECT 1
// keeps caller batches structurally identical; no SQL remains in TypeScript.
write5:async(facts:{value1:string;value2:string;value3:string;value4:string;value5:number;})=>nativeOwnerWrite('wp_reminder_delivery_upsert',[facts.value1,facts.value2,facts.value3,facts.value4,facts.value5],`wp-reminder-5:${facts.value2}:${facts.value3}:${facts.value4}`,{companyId:facts.value1},()=>db.prepare(noopSql)),
read6:async(facts:{value1:number;value2:string;value3:string;value4:string;value5:number;})=>nativeOwnerWriteRow('wp_reminder_delivery_claim',[facts.value1,facts.value2,facts.value3,facts.value4,facts.value5],`wp-reminder-6:${facts.value2}:${facts.value3}:${facts.value4}:${facts.value5}`,{companyId:facts.value2})};}
