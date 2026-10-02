import "server-only";
import type { CalendarItem, CalendarQuery } from "@/lib/calendar";
import { addDays } from "@/lib/people-roster";
import type { SqlDatabase } from "../runtime/types";
import { participantSql } from "../approvals";
import { rows, idList, inList, type CalendarAccess } from "./access";
export async function leaveItems(
  db: SqlDatabase,
  a: CalendarAccess,
  q: CalendarQuery,
  from: string,
  to: string,
  limit: number,
): Promise<CalendarItem[]> {
  const person =
    q.person ?? (q.mode === "mine" ? (a.staff?.person_id ?? "none") : null);
  // No getLeave/approvalView here: those initialize legacy routing. Calendar reads never write.
  const leave = await rows<{
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
  }>(
    db,
    `WITH visible AS (SELECT l.id,coalesce(l.person_id,e.person_id) person_id,p.legal_name name,l.start_date,l.end_date,l.days,l.state,l.version,coalesce(nullif(l.branch_id,''),nullif(e.branch_id,'')) branch_id,coalesce(nullif(l.department_id,''),nullif(e.department_id,'')) department_id,s.leave_date,s.segment,coalesce(l.calculation_snapshot->'policy'->>'timezone',v.timezone,'UTC') timezone,
 (l.created_by=? OR l.decided_by=? OR ${participantSql("leave", "l", "?", "?")}) full_detail
 FROM leave_requests l LEFT JOIN employees e ON e.id=l.employee_id AND e.company_id=l.company_id JOIN hr_people p ON p.id=coalesce(l.person_id,e.person_id) AND p.company_id=l.company_id
 LEFT JOIN leave_day_segments s ON s.request_id=l.id AND s.company_id=l.company_id LEFT JOIN leave_policy_versions v ON v.id=l.policy_version_id
 WHERE l.company_id=? AND coalesce(s.leave_date,l.start_date)<? AND coalesce(s.leave_date::date,l.end_date::date,l.start_date::date+(l.days::int-1))>=?::date)
 SELECT * FROM visible l WHERE ${inList("coalesce(l.department_id,l.branch_id,?)")} AND (full_detail OR (? AND state='approved')) AND state!='draft'
 AND (?::text IS NULL OR person_id=?) AND (?='' OR name ILIKE '%'||?||'%' OR 'Absence' ILIKE '%'||?||'%') ORDER BY id,leave_date LIMIT ?`,
    a.actor.userId,
    a.actor.userId,
    a.actor.userId,
    a.actor.assignmentId,
    a.actor.companyId,
    to,
    from,
    a.actor.companyId,
    idList(a.scopeIds),
    a.availability,
    person,
    person,
    q.search,
    q.search,
    q.search,
    limit,
  );
  const absences: CalendarItem[] = leave.map((l) => ({
    id: `leave:${l.id}${l.leave_date ? `:${l.leave_date}` : ''}`,
    source: "leave",
    sourceId: l.id,
    kind: "leave",
    title: `${l.name} · Unavailable`,
    allDay: true,
    startDate: l.leave_date ?? l.start_date,
    endDate: addDays(l.leave_date ?? l.end_date ?? addDays(l.start_date, l.days - 1), 1),
    startsAt: null,
    endsAt: null,
    timezone: l.timezone,
    departmentId: l.department_id,
    branchId: l.branch_id,
    participants: [{ id: l.person_id, name: l.name }],
    owner: l.name,
    status: l.state,
    version: l.full_detail ? l.version : 0,
    permittedActions: l.full_detail ? ["leave.open"] : [],
    ...(l.full_detail
      ? { href: `/tasks/people_payroll/${encodeURIComponent(l.id)}` }
      : { redacted: true }),
    warnings: [],
    history: [],
  }));
  const holidays = await rows<{id:string;name:string;holiday_date:string;timezone:string;calendar:string;participants:string|{id:string;name:string}[];department_id:string|null;branch_id:string|null}>(db,
    `SELECT h.id,h.name,h.holiday_date,c.timezone,c.name calendar,e.department_id,e.branch_id,jsonb_agg(DISTINCT jsonb_build_object('id',e.person_id,'name',p.legal_name)) participants
     FROM leave_holidays h JOIN leave_holiday_calendars c ON c.id=h.calendar_id
     JOIN leave_policy_assignments pa ON pa.holiday_calendar_id=c.id AND pa.company_id=c.company_id AND pa.effective_from<=h.holiday_date AND coalesce(pa.effective_to,'9999-12-31')>=h.holiday_date
     JOIN hr_engagements e ON e.id=pa.engagement_id AND e.company_id=pa.company_id JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
     WHERE c.company_id=? AND h.holiday_date>=? AND h.holiday_date<? AND e.start_date<=h.holiday_date AND (e.end_date IS NULL OR e.end_date>=h.holiday_date) AND p.merged_into_id IS NULL AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")}
     AND (?::text IS NULL OR e.person_id=?) AND (? OR e.person_id=?) AND (?='' OR h.name ILIKE '%'||?||'%' OR c.name ILIKE '%'||?||'%')
     GROUP BY h.id,h.name,h.holiday_date,c.timezone,c.name,e.department_id,e.branch_id ORDER BY h.id,e.department_id,e.branch_id LIMIT ?`,
    a.actor.companyId,from,to,idList(a.scopeIds),person,person,a.availability||a.rosterReader||a.lifecycleReader,a.staff?.person_id??"none",q.search,q.search,q.search,limit);
  const holidayItems: CalendarItem[] = holidays.map(holiday=>{
    const key = `${holiday.id}:${holiday.department_id??holiday.branch_id??a.actor.companyId}`;
    const participants = (typeof holiday.participants==="string"?JSON.parse(holiday.participants):holiday.participants) as {id:string;name:string}[];
    return {id:`holiday:${key}`,source:"leave",sourceId:holiday.id,kind:"holiday",title:holiday.name,description:holiday.calendar,allDay:true,startDate:holiday.holiday_date,endDate:addDays(holiday.holiday_date,1),startsAt:null,endsAt:null,timezone:holiday.timezone,departmentId:holiday.department_id,branchId:holiday.branch_id,participants:participants.sort((a,b)=>a.name.localeCompare(b.name)),owner:null,status:"holiday",version:0,permittedActions:[],warnings:[],history:[]};
  });
  return [...absences,...holidayItems];
}
