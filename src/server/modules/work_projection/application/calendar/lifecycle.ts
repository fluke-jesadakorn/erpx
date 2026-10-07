import { clearancePermissions, type ClearanceKind } from "@/lib/retirement";
import type { CalendarItem, CalendarQuery } from "@/server/modules/work_projection/public/rules/calendar-domain";
import { addDays } from "@/server/modules/people_payroll/public/rules/people-roster-domain";
import { hasPermission } from "@/lib/permissions";
import type {CalendarAccess} from "../../domain/calendar/access";
const idList=(ids:string[])=>JSON.stringify(ids);
import type {Clock} from "../../../shared/application/ports";
export type CalendarLifecycleUnit={read1(facts:{value1:string;value2:string;value3:string;value4:boolean;value5:string;value6:string;value7:string | null;value8:boolean;value9:string;value10:number;}):Promise<{ id: string; title: string; kind: string; own_assignee: boolean; clearance_type: ClearanceKind | null; status: string; version: number; due_date: string | null; responsibility: string; privileged: boolean; assignee_id: string | null; assignee_name: string | null; completed_by: string | null; completed_at: number | null; case_id: string | null; person_id: string | null; person_name: string | null; branch_id: string | null; department_id: string | null; }[]>;read2(facts:{value1:string;value2:string;value3:number;value4:number;value5:string;value6:string;value7:string | null;value8:string;value9:number;}):Promise<{ id: string; kind: string; status: string; version: number; effective_at: number; last_day: string | null; departure_reason: string; timezone: string; person_id: string; person_name: string; branch_id: string | null; department_id: string | null; cutoff_at: number | null; directory_at: number | null; restored_at: number | null; }[]>;};
export function createCalendarLifecycle(ports:{clock:Clock;error(status:number,code:string,message:string):Error}){







async function lifecycleItems(
  db: CalendarLifecycleUnit,
  a: CalendarAccess,
  q: CalendarQuery,
  from: string,
  to: string,
  fromMs: number,
  toMs: number,
  limit: number,
): Promise<CalendarItem[]> {
  const person =
      q.person ?? (q.mode === "mine" ? (a.staff?.person_id ?? "none") : null),
    it =
      hasPermission(a.actor, "iam.directory.manage") &&
      !a.actor.branchId &&
      !a.actor.departmentId;
  const tasks = await db.read1({value1:a.actor.userId,value2:a.actor.companyId,value3:idList(a.scopeIds),value4:a.lifecycleReader,value5:from,value6:to,value7:person,value8:q.mode === "mine",value9:q.search,value10:limit});
  const items: CalendarItem[] = tasks.map((t) => ({
    id: `task:${t.id}`,
    source: "lifecycle",
    sourceId: t.id,
    kind: "task",
    title: t.title,
    description: t.person_name ? `Preparation for ${t.person_name}` : undefined,
    allDay: true,
    startDate: t.due_date,
    endDate: t.due_date ? addDays(t.due_date, 1) : null,
    startsAt: null,
    endsAt: null,
    timezone: "Asia/Bangkok",
    departmentId: t.department_id,
    branchId: t.branch_id,
    participants: t.person_id
      ? [{ id: t.person_id, name: t.person_name ?? "Staff member" }]
      : [],
    owner:
      t.assignee_name ?? (t.assignee_id ? "Assigned colleague" : "Unassigned"),
    status: t.status,
    version: t.version,
    permittedActions: [
      ...(t.status === "pending" &&
      (t.clearance_type ? hasPermission(a.actor,"people.manage") || !!clearancePermissions[t.clearance_type] && hasPermission(a.actor,clearancePermissions[t.clearance_type]!) : t.privileged ? it : it || hasPermission(a.actor, "organization.manage"))
        ? ["task.plan"]
        : []),
      ...(t.status === "pending" &&
      (t.clearance_type ? hasPermission(a.actor,"people.manage") || !!clearancePermissions[t.clearance_type] && hasPermission(a.actor,clearancePermissions[t.clearance_type]!) : t.privileged ? it : it || hasPermission(a.actor, "organization.manage"))
        ? ["task.reschedule"]
        : []),
      ...(t.status === "pending" &&
      (!t.clearance_type && (t.privileged
        ? it && t.kind !== "position"
        : it || (!!a.staff && t.own_assignee)))
        ? ["task.complete"]
        : []),
      ...((a.lifecycleReader || t.clearance_type && t.own_assignee) && t.case_id ? ["lifecycle.open"] : []),
    ],
    ...((a.lifecycleReader || t.clearance_type && t.own_assignee) && t.case_id
      ? { href: t.clearance_type ? `/people/clearance/${encodeURIComponent(t.case_id)}` : `/people?case=${encodeURIComponent(t.case_id)}` }
      : {}),
    warnings:
      t.status === "pending" &&
      t.due_date &&
      t.due_date <
        new Intl.DateTimeFormat("en-CA", {
          timeZone: "Asia/Bangkok",
          year: "numeric",
          month: "2-digit",
          day: "2-digit",
        }).format(new Date(ports.clock.now()))
        ? ["Overdue preparation task"]
        : [],
    history: t.completed_at
      ? [
          {
            action: "Completed",
            actor: t.completed_by ?? "Authorized colleague",
            at: t.completed_at,
          },
        ]
      : [],
    details: {
      ...(a.lifecycleReader ? { caseId: t.case_id ?? undefined } : {}),
      responsibility: t.responsibility,
      assigneeId: t.assignee_id,
      privileged: t.privileged,
    },
  }));
  if (!a.lifecycleReader) return items;
  const cases = await db.read2({value1:a.actor.companyId,value2:idList(a.scopeIds),value3:fromMs,value4:toMs,value5:from,value6:to,value7:person,value8:q.search,value9:limit});
  for (const c of cases) {
    const base: CalendarItem = {
      id: `lifecycle:${c.id}`,
      source: "lifecycle",
      sourceId: c.id,
      kind: "start",
      title: `${c.person_name} · Start date`,
      allDay: false,
      startDate: null,
      endDate: null,
      startsAt: c.effective_at,
      endsAt: c.effective_at + 1,
      timezone: c.timezone,
      departmentId: c.department_id,
      branchId: c.branch_id,
      participants: [{ id: c.person_id, name: c.person_name }],
      owner: c.person_name,
      status: c.status,
      version: c.version,
      permittedActions: ["lifecycle.open"],
      href: `/people?case=${encodeURIComponent(c.id)}`,
      warnings: [],
      history: [],
    };
    if (
      c.kind === "onboarding" &&
      c.effective_at >= fromMs &&
      c.effective_at < toMs
    )
      items.push({
        ...base,
        id: `lifecycle:${c.id}:start`,
        warnings:
          c.effective_at > ports.clock.now() &&
          c.effective_at < ports.clock.now() + 7 * 86400000
            ? ["Starting within 7 days"]
            : [],
      });
    if (c.last_day && c.last_day >= from && c.last_day < to)
      items.push({
        ...base,
        id: `lifecycle:${c.id}:last-day`,
        kind: "last_day",
        title: `${c.person_name} · Last working day`,
        allDay: true,
        startDate: c.last_day,
        endDate: addDays(c.last_day, 1),
        startsAt: null,
        endsAt: null,
        warnings:
          c.last_day >= from && c.last_day < addDays(from, 7)
            ? ["Approaching departure"]
            : [],
      });
    if (c.cutoff_at && c.cutoff_at >= fromMs && c.cutoff_at < toMs)
      items.push({
        ...base,
        id: `lifecycle:${c.id}:cutoff`,
        kind: "cutoff",
        title: `${c.person_name} · ${c.departure_reason === "retirement" ? "Retirement & access cutoff" : "ERPX access cutoff"}`,
        startsAt: c.cutoff_at,
        endsAt: c.cutoff_at + 1,
        status: c.restored_at
          ? "restored"
          : c.cutoff_at <= ports.clock.now()
            ? "effective"
            : "scheduled",
        description:
          "ERP access ends at this time independently of checklist and directory work.",
      });
    if (c.directory_at && c.directory_at >= fromMs && c.directory_at < toMs)
      items.push({
        ...base,
        id: `lifecycle:${c.id}:directory`,
        kind: "directory",
        title: `${c.person_name} · Verified directory operation`,
        startsAt: c.directory_at,
        endsAt: c.directory_at + 1,
        status: "verified",
        description:
          "A directory disable operation was verified. This does not change the ERPX access cutoff.",
      });
  }
  return items;
}

return {lifecycleItems};}
