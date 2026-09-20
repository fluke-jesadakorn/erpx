import "server-only";
import type { CalendarItem, CalendarQuery } from "@/lib/calendar";
import { addDays } from "@/lib/people-roster";
import { hasPermission } from "@/lib/permissions";
import type { SqlDatabase } from "../runtime/types";
import type { WorkContext } from "../authorization";
import {
  calendarAccess,
  rows,
  idList,
  inList,
  type CalendarAccess,
} from "./access";
export type AnnouncementRow = {
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  title: string;
  content: string;
  start_date: string;
  end_date: string;
  status: string;
  version: number;
  created_by: string;
  created_at: number;
  updated_at: number;
};
export function announcementAudience(a: CalendarAccess) {
  const ids = new Set(a.scopeIds);
  // A scoped workspace also sees notices addressed to its ancestors.
  const roots = a.actor.departmentId
    ? [a.actor.departmentId]
    : a.actor.branchId
      ? [a.actor.branchId]
      : [];
  for (const root of roots) {
    let next: string | null = root;
    const seen = new Set<string>();
    while (next && !seen.has(next)) {
      seen.add(next);
      ids.add(next);
      next = a.nodes.find((n) => n.id === next)?.parentId ?? null;
    }
  }
  return [...ids];
}
export async function announcementItems(
  db: SqlDatabase,
  a: CalendarAccess,
  q: CalendarQuery,
  from: string,
  to: string,
  limit: number,
  publishedOnly = false,
): Promise<CalendarItem[]> {
  const manage = hasPermission(a.actor, "calendar.announcements.publish"),
    audience = announcementAudience(a);
  const records = await rows<AnnouncementRow & { owner_name: string }>(
    db,
    `SELECT n.*,coalesce(p.legal_name,'Publisher') owner_name FROM calendar_announcements n LEFT JOIN hr_people p ON p.user_id=n.created_by AND p.company_id=n.company_id AND p.merged_into_id IS NULL
 WHERE n.company_id=? AND ${inList("coalesce(n.department_id,n.branch_id,n.company_id)")} AND (n.status='published' OR (? AND ${inList("coalesce(n.department_id,n.branch_id,n.company_id)")}))
 AND n.start_date<? AND n.end_date>=? AND (?='' OR n.title ILIKE '%'||?||'%' OR n.content ILIKE '%'||?||'%') ORDER BY n.id LIMIT ?`,
    a.actor.companyId,
    idList(audience),
    manage && !publishedOnly,
    idList(a.scopeIds),
    to,
    from,
    q.search,
    q.search,
    q.search,
    limit,
  );
  return records.map((r) => ({
    id: `announcement:${r.id}`,
    source: "announcements",
    sourceId: r.id,
    kind: "announcement",
    title: r.title,
    description: r.content,
    allDay: true,
    startDate: r.start_date,
    endDate: addDays(r.end_date, 1),
    startsAt: null,
    endsAt: null,
    timezone: "Asia/Bangkok",
    departmentId: r.department_id,
    branchId: r.branch_id,
    participants: [],
    owner: r.owner_name,
    status: r.status,
    version: r.version,
    permittedActions:
      manage &&
      a.scopeIds.includes(r.department_id ?? r.branch_id ?? a.actor.companyId)
        ? [
            "announcement.save",
            "announcement.reschedule",
            ...(r.status !== "published"
              ? ["announcement.published"]
              : ["announcement.withdrawn"]),
          ]
        : [],
    warnings: [],
    history: [],
  }));
}
// Feed and calendar call the same adapter and audience predicate; there is no copied social post.
export async function publishedAnnouncements(
  db: SqlDatabase,
  actor: WorkContext,
  day: string,
) {
  const { calendarQuerySchema } = await import("@/lib/calendar");
  return announcementItems(
    db,
    await calendarAccess(db, actor),
    calendarQuerySchema.parse({ date: day }),
    day,
    addDays(day, 1),
    30,
    true,
  );
}
