type CalendarReadResult<Row>={results:Row[]};
import type { CalendarItem, CalendarQuery } from "@/server/modules/work_projection/public/rules/calendar-domain";
import { hasPermission } from "@/lib/identity_access/permissions";
import { expandCalendarSeries, type RecurrenceRule } from "@/lib/work_projection/calendar-recurrence";
import { localDateTimeToInstant, localMinuteAt } from "@/lib/work_projection/calendar-timeline";
import {withinScope} from "../../domain/calendar/access";
import type {CalendarAccess} from "../../domain/calendar/access";
const idList=(ids:string[])=>JSON.stringify(ids);
import type {AppointmentRow} from "../../domain/calendar/appointments";
import type {Clock} from "../../../shared/application/ports";
export type CalendarAppointmentsUnit={read1(facts:{value1:boolean;value2:string;value3:string | null;value4:string;value5:string;value6:number;value7:number;value8:boolean;value9:string | null;value10:"mine" | "team";value11:string | null;value12:string;value13:number;}):Promise<(AppointmentRow & { full_detail: boolean; organizer_name: string | null; })[]>;read2(facts:{value1:string;value2:string;}):Promise<{ appointment_id: string; user_id: string; person_id: string | null; name: string | null; in_scope: boolean; attendance: "organizer" | "required" | "optional"; response: "pending" | "accepted" | "tentative" | "declined"; }[]>;read3(facts:{value1:string;value2:string;}):Promise<CalendarReadResult<{ appointment_id: string; frequency: RecurrenceRule["frequency"]; interval_count: number; weekdays: string; end_kind: RecurrenceRule["endKind"]; occurrence_count: number | null; until_date: string | null; }>>;read4(facts:{value1:string;value2:string;}):Promise<CalendarReadResult<{ appointment_id: string; occurrence_key: string; start_local: string | null; end_local: string | null; cancelled: boolean; }>>;read5(facts:{value1:string;value2:string;}):Promise<CalendarReadResult<{ appointment_id: string; occurrence_key: string; user_id: string; response: "accepted" | "tentative" | "declined"; }>>;read6(facts:{value1:string;value2:string;value3:number;value4:number;value5:string;}):Promise<unknown>;read7(facts:{value1:string;value2:string;value3:number;value4:string;value5:number;}):Promise<unknown>;read8(facts:{value1:string;value2:string;value3:number;value4:number;value5:string;}):Promise<unknown>;read9(facts:{value1:string[];}):Promise<CalendarReadResult<{id:string;name:string}>>;};
export function createCalendarAppointments(ports:{clock:Clock;error(status:number,code:string,message:string):Error}){









async function appointmentRows(
  db: CalendarAppointmentsUnit,
  a: CalendarAccess,
  q: CalendarQuery,
  from: number,
  to: number,
  limit: number,
) {
  const manager = hasPermission(a.actor, "calendar.appointments.manage");
  // Invitees can open their own meeting even without an HR Person or a placement
  // in the organizer's selected workspace. Managers still receive only Busy.
  return db.read1({value1:manager,value2:idList(a.scopeIds),value3:a.staff?.person_id ?? null,value4:a.actor.userId,value5:a.actor.companyId,value6:to,value7:from,value8:a.availability,value9:q.person??null,value10:q.mode??"mine",value11:a.staff?.person_id??null,value12:q.search,value13:limit});
}
async function appointmentItems(
  db: CalendarAppointmentsUnit,
  a: CalendarAccess,
  q: CalendarQuery,
  from: number,
  to: number,
  limit: number,
): Promise<CalendarItem[]> {
  const result = await appointmentRows(db, a, q, from, to, limit),
    ids = result.map((x) => x.id);
  const participants = ids.length
    ? await db.read2({value1:idList(a.scopeIds),value2:idList(ids)})
    : [];
  const missingNames=[...new Set(participants.filter(p=>!p.name).map(p=>p.user_id))];
  const names=new Map<string,string>();
  if(missingNames.length){

    for(let index=0;index<missingNames.length;index+=100){
      const batch=missingNames.slice(index,index+100);
      const accounts=(await db.read9({value1:batch})).results;
      for(const account of accounts)names.set(account.id,account.name);
    }
  }
  const series=ids.length?(await db.read3({value1:a.actor.companyId,value2:idList(ids)})).results:[];
  const seriesById=new Map(series.map(row=>[row.appointment_id,row]));
  const exceptions=ids.length?(await db.read4({value1:a.actor.companyId,value2:idList(ids)})).results:[];
  const exceptionById=new Map(exceptions.map(row=>[`${row.appointment_id}:${row.occurrence_key}`,row]));
  const responses=ids.length?(await db.read5({value1:a.actor.companyId,value2:idList(ids)})).results:[];
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
                ...(r.starts_at <= ports.clock.now()
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
async function appointmentWarningMessages(
  db: CalendarAppointmentsUnit,
  companyId: string,
  ids: string[],
  start: number,
  end: number,
  zone: string,
  except: string,
  userIds: string[] = [],
) {
  const overlap = await db.read6({value1:companyId,value2:except,value3:end,value4:start,value5:idList(ids)});
  const leave = await db.read7({value1:companyId,value2:idList(ids),value3:end,value4:zone,value5:start});
  const accountOverlap=userIds.length?await db.read8({value1:companyId,value2:except,value3:end,value4:start,value5:idList(userIds)}):null;
  return [
    ...(overlap||accountOverlap ? ["An attendee is unavailable during this time."] : []),
    ...(leave ? ["An attendee has approved absence during this time."] : []),
  ];
}

return {appointmentRows,appointmentItems,appointmentWarningMessages};}
