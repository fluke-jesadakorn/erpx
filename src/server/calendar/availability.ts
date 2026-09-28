import "server-only";
import type {SqlDatabase} from "../runtime/types";
import type {WorkContext} from "../authorization";
import {ApiError} from "../http";
import {expandCalendarSeries,type RecurrenceRule} from "@/lib/calendar-recurrence";
import {localMinuteAt} from "@/lib/calendar-timeline";
import {localDateTimeToInstant} from "@/lib/calendar-timeline";
import {addDays} from "@/lib/people-roster";

/** The response contains busy intervals only. Titles, guests and reasons never leave this service. */
export async function calendarAvailability(db:SqlDatabase,actor:WorkContext,ids:string[],from:number,to:number){
  const userIds=[...new Set(ids)].slice(0,100);
  if(!userIds.length||!Number.isSafeInteger(from)||!Number.isSafeInteger(to)||to<=from||to-from>31*86400000)
    throw new ApiError(422,"AVAILABILITY_RANGE","Choose people and a range of up to 31 days.");
  const now=Date.now();
  const active=(await db.prepare(`SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND user_id IN (${userIds.map(()=>"?").join(",")})
    AND revoked_at IS NULL AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)`)
    .bind(actor.companyId,...userIds,now,now).all<{user_id:string}>()).results;
  if(active.length!==userIds.length)throw new ApiError(404,"INVITEE_UNAVAILABLE","Choose active Company accounts.");
  const appointments=(await db.prepare(`SELECT a.id,a.starts_at,a.ends_at,a.timezone,a.status,a.all_day,
    s.frequency,s.interval_count,s.weekdays,s.end_kind,s.occurrence_count,s.until_date,t.user_id,t.response
    FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id
    LEFT JOIN calendar_series s ON s.appointment_id=a.id
    WHERE a.company_id=? AND a.status='scheduled' AND a.starts_at<? AND t.response!='declined'
    AND t.user_id IN (${userIds.map(()=>"?").join(",")})`)
    .bind(actor.companyId,to,...userIds).all<{id:string;starts_at:number;ends_at:number;timezone:string;frequency:RecurrenceRule["frequency"]|null;
      interval_count:number|null;weekdays:string|null;end_kind:RecurrenceRule["endKind"]|null;occurrence_count:number|null;until_date:string|null;user_id:string}>()).results;
  const intervals:{userId:string;startsAt:number;endsAt:number}[]=[];
  for(const row of appointments){
    if(!row.frequency){if(row.ends_at>from)intervals.push({userId:row.user_id,startsAt:Math.max(from,row.starts_at),endsAt:Math.min(to,row.ends_at)});continue;}
    const start=localMinuteAt(row.starts_at,row.timezone),end=localMinuteAt(row.ends_at,row.timezone);
    const rule:RecurrenceRule={frequency:row.frequency,intervalCount:row.interval_count!,weekdays:row.weekdays?row.weekdays.split(",").map(Number):[],
      endKind:row.end_kind!,occurrenceCount:row.occurrence_count,untilDate:row.until_date};
    const considered=new Set<string>();
    for(const occurrence of expandCalendarSeries(`${start.date}T${start.time}`,`${end.date}T${end.time}`,row.timezone,rule,from,to,500)){
      considered.add(occurrence.key);
      const exception=await db.prepare("SELECT start_local,end_local,cancelled FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND occurrence_key=?")
        .bind(row.id,actor.companyId,occurrence.key).first<{start_local:string|null;end_local:string|null;cancelled:boolean}>();
      if(exception?.cancelled)continue;
      const response=await db.prepare("SELECT response FROM calendar_occurrence_responses WHERE appointment_id=? AND company_id=? AND occurrence_key=? AND user_id=?")
        .bind(row.id,actor.companyId,occurrence.key,row.user_id).first<{response:string}>();
      if(response?.response==="declined")continue;
      let startsAt=occurrence.startsAt,endsAt=occurrence.endsAt;
      if(exception?.start_local&&exception.end_local){try{startsAt=localDateTimeToInstant(exception.start_local,row.timezone);endsAt=localDateTimeToInstant(exception.end_local,row.timezone)}catch{continue}}
      if(startsAt<to&&endsAt>from)intervals.push({userId:row.user_id,startsAt:Math.max(from,startsAt),endsAt:Math.min(to,endsAt)});
    }
    const moved=(await db.prepare("SELECT occurrence_key,start_local,end_local FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND cancelled=false AND start_local IS NOT NULL AND end_local IS NOT NULL")
      .bind(row.id,actor.companyId).all<{occurrence_key:string;start_local:string;end_local:string}>()).results;
    for(const change of moved){
      if(considered.has(change.occurrence_key))continue;
      const response=await db.prepare("SELECT response FROM calendar_occurrence_responses WHERE appointment_id=? AND company_id=? AND occurrence_key=? AND user_id=?")
        .bind(row.id,actor.companyId,change.occurrence_key,row.user_id).first<{response:string}>();
      if(response?.response==="declined")continue;
      try{const startsAt=localDateTimeToInstant(change.start_local,row.timezone),endsAt=localDateTimeToInstant(change.end_local,row.timezone);
        if(startsAt<to&&endsAt>from)intervals.push({userId:row.user_id,startsAt:Math.max(from,startsAt),endsAt:Math.min(to,endsAt)});
      }catch{/* Invalid wall-clock exception is ignored. */}
    }
  }
  const leave=(await db.prepare(`SELECT DISTINCT p.user_id,d.leave_date::text leave_date
    FROM leave_requests l JOIN hr_people p ON p.id=l.person_id AND p.company_id=l.company_id
    CROSS JOIN LATERAL (
      SELECT s.leave_date::date leave_date FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id
      UNION ALL
      SELECT generate_series(l.start_date::date,coalesce(l.end_date::date,l.start_date::date+l.days::int-1),interval '1 day')::date
      WHERE NOT EXISTS(SELECT 1 FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id)
    ) d
    WHERE l.company_id=? AND l.state='approved' AND p.user_id IN (${userIds.map(()=>"?").join(",")})
    AND d.leave_date BETWEEN ?::date AND ?::date`)
    .bind(actor.companyId,...userIds,new Date(from-86400000).toISOString().slice(0,10),new Date(to+86400000).toISOString().slice(0,10))
    .all<{user_id:string;leave_date:string}>()).results;
  for(const row of leave){
    try{const start=localDateTimeToInstant(`${row.leave_date}T00:00`,"Asia/Bangkok"),end=localDateTimeToInstant(`${addDays(row.leave_date,1)}T00:00`,"Asia/Bangkok");
      if(start<to&&end>from)intervals.push({userId:row.user_id,startsAt:Math.max(from,start),endsAt:Math.min(to,end)});
    }catch{/* Invalid local midnight is omitted. */}
  }
  return {intervals:intervals.sort((a,b)=>a.startsAt-b.startsAt),from,to};
}
