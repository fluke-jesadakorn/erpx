import "server-only";
import type { CalendarItem, CalendarQuery } from "@/lib/calendar";
import { hasPermission } from "@/lib/permissions";
import type { SqlDatabase } from "../runtime/types";
import { rows, idList, inList, type CalendarAccess } from "./access";
export async function rosterItems(
  db: SqlDatabase,
  a: CalendarAccess,
  q: CalendarQuery,
  from: number,
  to: number,
  limit: number,
): Promise<CalendarItem[]> {
  const person =
    q.person ?? (q.mode === "mine" ? (a.staff?.person_id ?? "none") : null);
  const shifts = await rows<{
    id: string;
    name: string;
    local_date: string;
    starts_at: number;
    ends_at: number;
    capacity: number;
    version: number;
    week_id: string;
    week_start: string;
    timezone: string;
    state: string;
    department_id: string;
    branch_id: string | null;
    owner_name: string;
  }>(
    db,
    `SELECT s.id,s.name,s.local_date,s.starts_at,s.ends_at,s.capacity,s.version,s.week_id,w.week_start,w.timezone,w.state,w.department_id,w.branch_id,coalesce(p.legal_name,'Roster manager') owner_name
 FROM people_roster_shifts s JOIN people_roster_weeks w ON w.id=s.week_id LEFT JOIN hr_people p ON p.user_id=w.created_by AND p.company_id=w.company_id AND p.merged_into_id IS NULL
 WHERE w.company_id=? AND ${inList("w.department_id")} AND (? OR w.state='published') AND s.state='active' AND s.starts_at<? AND s.ends_at>?
 AND (?::text IS NULL OR EXISTS(SELECT 1 FROM people_roster_assignments ra WHERE ra.shift_id=s.id AND ra.state='assigned' AND ra.person_id=?))
 AND (?='' OR s.name ILIKE '%'||?||'%' OR EXISTS(SELECT 1 FROM people_roster_assignments ra JOIN hr_people hp ON hp.id=ra.person_id WHERE ra.shift_id=s.id AND ra.state='assigned' AND hp.legal_name ILIKE '%'||?||'%')) ORDER BY s.id LIMIT ?`,
    a.actor.companyId,
    idList(a.rosterIds),
    a.rosterReader,
    to,
    from,
    person,
    person,
    q.search,
    q.search,
    q.search,
    limit,
  );
  const assignments = shifts.length
    ? await rows<{ shift_id: string; id: string; name: string }>(
        db,
        `SELECT ra.shift_id,p.id,p.legal_name name FROM people_roster_assignments ra JOIN hr_people p ON p.id=ra.person_id WHERE ra.state='assigned' AND ${inList("ra.shift_id")} ORDER BY p.legal_name`,
        idList(shifts.map((s) => s.id)),
      )
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
        ...(hasPermission(a.actor, "roster.manage") && s.starts_at > Date.now()
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
