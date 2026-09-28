import "server-only";
import type { CalendarItem, CalendarQuery } from "@/lib/calendar";
import type { SqlDatabase } from "../runtime/types";
import { hasPermission } from "@/lib/permissions";
import { expandCalendarSeries, type RecurrenceRule } from "@/lib/calendar-recurrence";
import { localDateTimeToInstant, localMinuteAt } from "@/lib/calendar-timeline";
import { runtimeEnv } from "../runtime";
import {
  rows,
  one,
  idList,
  inList,
  withinScope,
  type CalendarAccess,
} from "./access";
export type AppointmentRow = {
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  organizer_person_id: string|null;
  title: string;
  description: string;
  starts_at: number;
  ends_at: number;
  timezone: string;
  location: string;
  meeting_url: string;
  private: boolean;
  guests: NonNullable<CalendarItem["details"]>["guests"];
  status: string;
  version: number;
  created_by: string;
  created_at: number;
  updated_at: number;
  all_day: boolean;
  start_date: string | null;
  end_date: string | null;
};
export async function appointmentRows(
  db: SqlDatabase,
  a: CalendarAccess,
  q: CalendarQuery,
  from: number,
  to: number,
  limit: number,
) {
  const manager = hasPermission(a.actor, "calendar.appointments.manage");
  // Invitees can open their own meeting even without an HR Person or a placement
  // in the organizer's selected workspace. Managers still receive only Busy.
  return rows<
    AppointmentRow & { full_detail: boolean; organizer_name: string|null }
  >(
    db,
    `WITH candidates AS (
 SELECT x.*,p.legal_name organizer_name,
 ((? AND ${inList("coalesce(x.department_id,x.branch_id,x.company_id)")}) OR x.organizer_person_id=? OR EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=?)) full_detail
 FROM calendar_appointments x LEFT JOIN hr_people p ON p.id=x.organizer_person_id AND p.company_id=x.company_id
 WHERE x.company_id=? AND x.starts_at<? AND (x.ends_at>? OR EXISTS(SELECT 1 FROM calendar_series s WHERE s.appointment_id=x.id))
 AND (${inList("coalesce(x.department_id,x.branch_id,x.company_id)")} OR EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=?) OR EXISTS(SELECT 1 FROM calendar_appointment_participants ap JOIN hr_engagements e ON e.person_id=ap.person_id AND e.company_id=ap.company_id WHERE ap.appointment_id=x.id AND e.status='active' AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")})))
 SELECT x.* FROM candidates x WHERE (full_detail OR (? AND x.status='scheduled'))
 AND ((?::text IS NOT NULL AND EXISTS(SELECT 1 FROM calendar_appointment_participants ap WHERE ap.appointment_id=x.id AND ap.person_id=?))
   OR (?::text IS NULL AND (?='team' OR EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=?) OR x.organizer_person_id=?)))
 AND (?='' OR (CASE WHEN full_detail THEN x.title ELSE 'Busy' END) ILIKE '%'||?||'%') ORDER BY x.id LIMIT ?`,
    manager,
    idList(a.scopeIds),
    a.staff?.person_id ?? null,
    a.actor.userId,
    a.actor.companyId,
    to,
    from,
    idList(a.scopeIds),
    a.actor.userId,
    idList(a.scopeIds),
    a.availability,
    q.person??null,
    q.person??null,
    q.person??null,
    q.mode??"mine",
    a.actor.userId,
    a.staff?.person_id??null,
    q.search,
    q.search,
    limit,
  );
}
export async function appointmentItems(
  db: SqlDatabase,
  a: CalendarAccess,
  q: CalendarQuery,
  from: number,
  to: number,
  limit: number,
): Promise<CalendarItem[]> {
  const result = await appointmentRows(db, a, q, from, to, limit),
    ids = result.map((x) => x.id);
  const participants = ids.length
    ? await rows<{
        appointment_id: string;
        user_id: string;
        person_id: string | null;
        name: string | null;
        in_scope: boolean;
        attendance:"organizer"|"required"|"optional";
        response:"pending"|"accepted"|"tentative"|"declined";
      }>(
        db,
        `SELECT ap.appointment_id,ap.user_id,ap.person_id,p.legal_name name,EXISTS(SELECT 1 FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")}) in_scope,ap.attendance,ap.response
          FROM calendar_attendees ap LEFT JOIN hr_people p ON p.id=ap.person_id AND p.company_id=ap.company_id WHERE ${inList("ap.appointment_id")} ORDER BY p.legal_name,ap.user_id`,
        idList(a.scopeIds),
        idList(ids),
      )
    : [];
  const missingNames=[...new Set(participants.filter(p=>!p.name).map(p=>p.user_id))];
  const names=new Map<string,string>();
  if(missingNames.length){
    const env=await runtimeEnv();
    for(let index=0;index<missingNames.length;index+=100){
      const batch=missingNames.slice(index,index+100);
      const accounts=(await env.CONTROL_DB.prepare(`SELECT id,name FROM "user" WHERE id IN (${batch.map(()=>"?").join(",")})`)
        .bind(...batch).all<{id:string;name:string}>()).results;
      for(const account of accounts)names.set(account.id,account.name);
    }
  }
  const series=ids.length?(await db.prepare(`SELECT * FROM calendar_series WHERE company_id=? AND ${inList("appointment_id")}`)
    .bind(a.actor.companyId,idList(ids)).all<{appointment_id:string;frequency:RecurrenceRule["frequency"];interval_count:number;weekdays:string;end_kind:RecurrenceRule["endKind"];occurrence_count:number|null;until_date:string|null}>()).results:[];
  const seriesById=new Map(series.map(row=>[row.appointment_id,row]));
  const exceptions=ids.length?(await db.prepare(`SELECT * FROM calendar_occurrence_exceptions WHERE company_id=? AND ${inList("appointment_id")}`)
    .bind(a.actor.companyId,idList(ids)).all<{appointment_id:string;occurrence_key:string;start_local:string|null;end_local:string|null;cancelled:boolean}>()).results:[];
  const exceptionById=new Map(exceptions.map(row=>[`${row.appointment_id}:${row.occurrence_key}`,row]));
  const responses=ids.length?(await db.prepare(`SELECT appointment_id,occurrence_key,user_id,response FROM calendar_occurrence_responses WHERE company_id=? AND ${inList("appointment_id")}`)
    .bind(a.actor.companyId,idList(ids)).all<{appointment_id:string;occurrence_key:string;user_id:string;response:"accepted"|"tentative"|"declined"}>()).results:[];
  const responseById=new Map(responses.map(row=>[`${row.appointment_id}:${row.occurrence_key}:${row.user_id}`,row.response]));
  return result
    .flatMap((r): CalendarItem[] => {
      // Nonparticipants receive only availability, even for a nonprivate meeting; guest contacts never leak.
      const full = r.full_detail,
        manage =
          withinScope(a, r.branch_id, r.department_id) &&
          (hasPermission(a.actor, "calendar.appointments.manage") ||
            r.organizer_person_id === a.staff?.person_id || r.created_by===a.actor.userId);
      const attendeeRows=participants.filter(p=>p.appointment_id===r.id);
      const own=attendeeRows.find(p=>p.user_id===a.actor.userId);
      const seriesRow=seriesById.get(r.id);
      const rule:RecurrenceRule|undefined=seriesRow?{frequency:seriesRow.frequency,intervalCount:seriesRow.interval_count,
        weekdays:seriesRow.weekdays?seriesRow.weekdays.split(",").map(Number):[],endKind:seriesRow.end_kind,
        occurrenceCount:seriesRow.occurrence_count,untilDate:seriesRow.until_date}:undefined;
      const start=localMinuteAt(r.starts_at,r.timezone),end=localMinuteAt(r.ends_at,r.timezone);
      const occurrences=rule?expandCalendarSeries(`${start.date}T${start.time}`,`${end.date}T${end.time}`,
        r.timezone,rule,from,to,limit):[{key:`${start.date}T${start.time}`,startsAt:r.starts_at,endsAt:r.ends_at}];
      const moved=rule?exceptions.filter(change=>change.appointment_id===r.id&&!change.cancelled&&change.start_local&&change.end_local&&
        !occurrences.some(occurrence=>occurrence.key===change.occurrence_key)).flatMap(change=>{
          try{const startsAt=localDateTimeToInstant(change.start_local!,r.timezone),endsAt=localDateTimeToInstant(change.end_local!,r.timezone);
            return startsAt<to&&endsAt>from?[{key:change.occurrence_key,startsAt,endsAt}]:[];
          }catch{return []}
        }):[];
      return [...occurrences,...moved].flatMap(occurrence=>{
        const change=exceptionById.get(`${r.id}:${occurrence.key}`);
        if(change?.cancelled)return [];
        let startsAt=occurrence.startsAt,endsAt=occurrence.endsAt;
        if(change?.start_local&&change.end_local){
          try{startsAt=localDateTimeToInstant(change.start_local,r.timezone);endsAt=localDateTimeToInstant(change.end_local,r.timezone);}
          catch{return [];}
        }
        if(startsAt>=to||endsAt<=from)return [];
        const myResponse=responseById.get(`${r.id}:${occurrence.key}:${a.actor.userId}`)??own?.response;
        if(q.mode==="mine"&&own?.attendance!=="organizer"&&myResponse==="declined")return [];
        const localStart=localMinuteAt(startsAt,r.timezone);
        const item:CalendarItem={
        id: rule?`appointment:${r.id}@${occurrence.key}`:`appointment:${r.id}`,
        source: "appointments",
        sourceId: r.id,
        ...(rule?{occurrenceId:occurrence.key,seriesId:r.id,recurrence:rule}:{}),
        ...(full&&myResponse?{myResponse}:{}),
        kind: "appointment",
        title: full ? r.title : "Busy",
        ...(full ? { description: r.description } : {}),
        allDay: r.all_day,
        startDate:r.all_day?localStart.date:null,
        endDate:r.all_day?localMinuteAt(endsAt,r.timezone).date:null,
        startsAt,
        endsAt,
        timezone: r.timezone,
        departmentId: full ? r.department_id : null,
        branchId: full ? r.branch_id : null,
        participants: attendeeRows.filter(p=>full||p.in_scope)
          .map(p=>({id:p.person_id??p.user_id,name:p.name??names.get(p.user_id)??"Colleague"})),
        owner: full ? r.organizer_name??"Organizer" : null,
        status: full
          ? r.status
          : r.status === "scheduled"
            ? "busy"
            : "unavailable",
        version: full ? r.version : 0,
        permittedActions:
          full && r.status === "scheduled"
            ? [...(manage?[
              "appointment.save",
                "appointment.reschedule",
                "appointment.cancelled",
                ...(r.starts_at <= Date.now()
                  ? ["appointment.completed", "appointment.no_show"]
                  : []),
              ]:[]),...(own&&own.attendance!=="organizer"?["appointment.respond","appointment.reminder.set"]:[])]
            : [],
        warnings: [],
        history: [],
        redacted: !full,
        ...(full
          ? {
              details: {
                location: r.location,
                meetingUrl: r.meeting_url,
                guests:
                  typeof r.guests === "string"
                    ? JSON.parse(r.guests)
                    : r.guests,
                private: r.private,
                organizerPersonId: r.organizer_person_id??undefined,
                attendees:attendeeRows.map(p=>({userId:p.user_id,name:p.name??names.get(p.user_id)??"Colleague",
                  attendance:p.attendance,response:responseById.get(`${r.id}:${occurrence.key}:${p.user_id}`)??p.response})),
              },
            }
          : {}),
        };
        return [item];
      });
    })
    .filter((r) => !r.redacted || r.status === "busy");
}
export async function appointmentWarningMessages(
  db: SqlDatabase,
  companyId: string,
  ids: string[],
  start: number,
  end: number,
  zone: string,
  except: string,
  userIds: string[] = [],
) {
  const overlap = await one(
    db,
    `SELECT 1 FROM calendar_appointments a JOIN calendar_appointment_participants p ON p.appointment_id=a.id WHERE a.company_id=? AND a.status='scheduled' AND a.id!=? AND a.starts_at<? AND a.ends_at>? AND ${inList("p.person_id")} LIMIT 1`,
    companyId,
    except,
    end,
    start,
    idList(ids),
  );
  const leave = await one(
    db,
    `SELECT 1 FROM leave_requests l LEFT JOIN employees e ON e.id=l.employee_id AND e.company_id=l.company_id WHERE l.company_id=? AND l.state='approved' AND ${inList("coalesce(l.person_id,e.person_id)")} AND EXISTS(SELECT 1 FROM (
 SELECT s.leave_date::date absence_date FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id
 UNION ALL SELECT generate_series(l.start_date::date,coalesce(l.end_date::date,l.start_date::date+l.days::int-1),interval '1 day')::date WHERE NOT EXISTS(SELECT 1 FROM leave_day_segments s WHERE s.request_id=l.id)
 ) absence WHERE absence_date<=(to_timestamp((?::bigint-1)/1000.0) AT TIME ZONE ?)::date AND absence_date>=(to_timestamp(?::bigint/1000.0) AT TIME ZONE ?)::date) LIMIT 1`,
    companyId,
    idList(ids),
    end,
    zone,
    start,
    zone,
  );
  const accountOverlap=userIds.length?await one(db,`SELECT 1 FROM calendar_appointments a JOIN calendar_attendees p ON p.appointment_id=a.id
    WHERE a.company_id=? AND a.status='scheduled' AND a.id!=? AND a.starts_at<? AND a.ends_at>?
    AND p.response!='declined' AND ${inList("p.user_id")} LIMIT 1`,companyId,except,end,start,idList(userIds)):null;
  return [
    ...(overlap||accountOverlap ? ["An attendee is unavailable during this time."] : []),
    ...(leave ? ["An attendee has approved absence during this time."] : []),
  ];
}
