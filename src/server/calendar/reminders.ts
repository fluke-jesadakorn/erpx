import "server-only";
import type {PostgresDatabase} from "../runtime/postgres";
import {expandCalendarSeries,type RecurrenceRule} from "@/lib/calendar-recurrence";
import {localDateTimeToInstant,localMinuteAt} from "@/lib/calendar-timeline";
import {publishInboxEvent} from "../inbox";

type Row={id:string;company_id:string;title:string;starts_at:number;ends_at:number;timezone:string;
 frequency:RecurrenceRule["frequency"]|null;interval_count:number|null;weekdays:string|null;end_kind:RecurrenceRule["endKind"]|null;
 occurrence_count:number|null;until_date:string|null;user_id:string;reminder_minutes:number;response:string};

/** Scheduled worker; the reminder ledger and event key make repeated runs safe. */
export async function drainCalendarReminders(db:PostgresDatabase,now=Date.now()){
  let after="",sent=0;
  for(;;){
    const rows=(await db.prepare(`SELECT a.id,a.company_id,a.title,a.starts_at,a.ends_at,a.timezone,
      s.frequency,s.interval_count,s.weekdays,s.end_kind,s.occurrence_count,s.until_date,
      t.user_id,t.reminder_minutes,t.response FROM calendar_appointments a
      JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id
      LEFT JOIN calendar_series s ON s.appointment_id=a.id
      WHERE a.status='scheduled' AND a.starts_at<? AND t.reminder_minutes IS NOT NULL AND t.response!='declined'
      AND (s.appointment_id IS NOT NULL OR a.ends_at>?) AND (a.id||':'||t.user_id)>?
      ORDER BY a.id,t.user_id LIMIT 200`)
      .bind(now+8*86400000,now-3600000,after).all<Row>()).results;
    if(!rows.length)break;
    for(const row of rows){
      after=`${row.id}:${row.user_id}`;
      const start=localMinuteAt(row.starts_at,row.timezone),end=localMinuteAt(row.ends_at,row.timezone);
      const occurrences=row.frequency?expandCalendarSeries(`${start.date}T${start.time}`,`${end.date}T${end.time}`,row.timezone,
        {frequency:row.frequency,intervalCount:row.interval_count!,weekdays:row.weekdays?row.weekdays.split(",").map(Number):[],
         endKind:row.end_kind!,occurrenceCount:row.occurrence_count,untilDate:row.until_date},now-3600000,now+8*86400000,1000):
        [{key:`${start.date}T${start.time}`,startsAt:row.starts_at,endsAt:row.ends_at}];
      if(row.frequency){
        const known=new Set(occurrences.map(value=>value.key));
        const moved=(await db.prepare(`SELECT occurrence_key,start_local,end_local FROM calendar_occurrence_exceptions
          WHERE appointment_id=? AND company_id=? AND cancelled=false AND start_local IS NOT NULL AND end_local IS NOT NULL`)
          .bind(row.id,row.company_id).all<{occurrence_key:string;start_local:string;end_local:string}>()).results;
        for(const change of moved){
          if(known.has(change.occurrence_key))continue;
          try{
            const startsAt=localDateTimeToInstant(change.start_local,row.timezone);
            if(startsAt<now-3600000||startsAt>=now+8*86400000)continue;
            occurrences.push({key:change.occurrence_key,startLocal:change.start_local,endLocal:change.end_local,
              startsAt,endsAt:localDateTimeToInstant(change.end_local,row.timezone)});
          }catch{/* Ignore a nonexistent local time. */}
        }
      }
      for(const occurrence of occurrences){
        const exception=await db.prepare("SELECT start_local,end_local,cancelled FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND occurrence_key=?")
          .bind(row.id,row.company_id,occurrence.key).first<{start_local:string|null;end_local:string|null;cancelled:boolean}>();
        if(exception?.cancelled)continue;
        let startsAt=occurrence.startsAt;
        if(exception?.start_local){try{startsAt=localDateTimeToInstant(exception.start_local,row.timezone)}catch{continue}}
        const response=await db.prepare("SELECT response FROM calendar_occurrence_responses WHERE appointment_id=? AND company_id=? AND occurrence_key=? AND user_id=?")
          .bind(row.id,row.company_id,occurrence.key,row.user_id).first<{response:string}>();
        if(response?.response==="declined")continue;
        const due=startsAt-row.reminder_minutes*60000;
        if(due>now||startsAt<now-3600000)continue;
        await db.transaction(async tx=>{
          await tx.prepare(`INSERT INTO calendar_reminder_deliveries(company_id,appointment_id,occurrence_key,user_id,due_at)
            VALUES(?,?,?,?,?) ON CONFLICT(appointment_id,occurrence_key,user_id) DO UPDATE SET
            due_at=excluded.due_at,delivered_at=CASE WHEN calendar_reminder_deliveries.due_at=excluded.due_at
              THEN calendar_reminder_deliveries.delivered_at ELSE NULL END`)
            .bind(row.company_id,row.id,occurrence.key,row.user_id,due).run();
          const claimed=await tx.prepare(`UPDATE calendar_reminder_deliveries SET delivered_at=?
            WHERE appointment_id=? AND occurrence_key=? AND user_id=? AND due_at=? AND delivered_at IS NULL
            RETURNING user_id`).bind(now,row.id,occurrence.key,row.user_id,due).first();
          if(!claimed)return;
          await publishInboxEvent(tx,{key:`reminder:${row.id}:${occurrence.key}:${row.user_id}:${due}`,companyId:row.company_id,
            sourceType:"calendar_appointment",sourceId:row.id,sourceVersion:1,category:"calendar",preview:`Meeting soon: ${row.title}`,
            recipientIds:[row.user_id],createdAt:now});
          sent++;
        });
      }
    }
    if(rows.length<200)break;
  }
  return sent;
}
