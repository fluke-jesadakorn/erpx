import type { CalendarItem, CalendarQuery } from "@/server/modules/work_projection/public/rules/calendar-domain";
import { hasPermission } from "@/lib/permissions";
import type {CalendarAccess} from "../../domain/calendar/access";
const idList=(ids:string[])=>JSON.stringify(ids);
import type {Clock} from "../../../shared/application/ports";
export type CalendarRosterUnit={read1(facts:{value1:string;value2:string;value3:boolean;value4:number;value5:number;value6:string | null;value7:string;value8:number;}):Promise<{ id: string; name: string; local_date: string; starts_at: number; ends_at: number; capacity: number; version: number; week_id: string; week_start: string; timezone: string; state: string; department_id: string; branch_id: string | null; owner_name: string; }[]>;read2(facts:{value1:string;}):Promise<{ shift_id: string; id: string; name: string; }[]>;};
export function createCalendarRoster(ports:{clock:Clock;error(status:number,code:string,message:string):Error}){





async function rosterItems(
  db: CalendarRosterUnit,
  a: CalendarAccess,
  q: CalendarQuery,
  from: number,
  to: number,
  limit: number,
): Promise<CalendarItem[]> {
  const person =
    q.person ?? (q.mode === "mine" ? (a.staff?.person_id ?? "none") : null);
  const shifts = await db.read1({value1:a.actor.companyId,value2:idList(a.rosterIds),value3:a.rosterReader,value4:to,value5:from,value6:person,value7:q.search,value8:limit});
  const assignments = shifts.length
    ? await db.read2({value1:idList(shifts.map((s) => s.id))})
    : [];
  return shifts.map((s) => {
    const pp = assignments
      .filter((x) => x.shift_id === s.id)
      .map((p) => ({ id: p.id, name: p.name }));
    return {
      id: `shift:${s.id}`,
      source: "roster",
      sourceId: s.id,
      kind: "shift",
      title: s.name,
      allDay: false,
      startDate: s.local_date,
      endDate: null,
      startsAt: s.starts_at,
      endsAt: s.ends_at,
      timezone: s.timezone,
      departmentId: s.department_id,
      branchId: s.branch_id,
      participants: pp,
      owner: s.owner_name,
      status: s.state,
      version: s.version,
      permittedActions: [
        "roster.open",
        ...(hasPermission(a.actor, "roster.manage") && s.starts_at > ports.clock.now()
          ? ["roster.manage", "shift.reschedule"]
          : []),
      ],
      href: `/people/roster?department=${encodeURIComponent(s.department_id)}&week=${s.week_start}`,
      warnings:
        s.capacity > pp.length
          ? [
              `${s.capacity - pp.length} unfilled shift ${s.capacity - pp.length === 1 ? "slot" : "slots"}`,
            ]
          : [],
      history: [],
      capacity: s.capacity,
      assigned: pp.length,
      details: {
        requiresReason: s.state === "published",
        published: s.state === "published",
      },
    };
  });
}

return {rosterItems};}
