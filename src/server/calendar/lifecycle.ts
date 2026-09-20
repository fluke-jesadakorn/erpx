import { clearancePermissions, type ClearanceKind } from "@/lib/retirement";
import "server-only";
import type { CalendarItem, CalendarQuery } from "@/lib/calendar";
import { addDays } from "@/lib/people-roster";
import { hasPermission } from "@/lib/permissions";
import type { SqlDatabase } from "../runtime/types";
import { rows, idList, inList, type CalendarAccess } from "./access";
export async function lifecycleItems(
  db: SqlDatabase,
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
  const tasks = await rows<{
    id: string;
    title: string;
    kind: string;
    own_assignee: boolean;
    clearance_type: ClearanceKind | null;
    status: string;
    version: number;
    due_date: string | null;
    responsibility: string;
    privileged: boolean;
    assignee_id: string | null;
    assignee_name: string | null;
    completed_by: string | null;
    completed_at: number | null;
    case_id: string | null;
    person_id: string | null;
    person_name: string | null;
    branch_id: string | null;
    department_id: string | null;
  }>(
    db,
    `SELECT t.*,iam_same_person(t.assignee_id,?,t.company_id) own_assignee,c.person_id,h.legal_name person_name,coalesce(e.branch_id,p.branch_id) branch_id,coalesce(e.department_id,p.department_id) department_id,assignee.legal_name assignee_name
 FROM people_preparation_tasks t LEFT JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id LEFT JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=t.company_id LEFT JOIN hr_people h ON h.id=c.person_id AND h.company_id=t.company_id LEFT JOIN organization_positions p ON p.id=t.position_id AND p.company_id=t.company_id LEFT JOIN hr_people assignee ON assignee.user_id=t.assignee_id AND assignee.company_id=t.company_id AND assignee.merged_into_id IS NULL
 WHERE t.company_id=? AND ${inList("coalesce(e.department_id,p.department_id,e.branch_id,p.branch_id,t.company_id)")} AND (? OR iam_same_person(t.assignee_id,?,t.company_id))
 AND (t.due_date>=? AND t.due_date<? OR t.status='pending' AND (t.due_date IS NULL OR t.due_date<?))
 AND (?::text IS NULL OR c.person_id=? OR (t.assignee_id=? AND ?))
 AND (?='' OR t.title ILIKE '%'||?||'%' OR h.legal_name ILIKE '%'||?||'%') ORDER BY t.id LIMIT ?`,
    a.actor.userId,
    a.actor.companyId,
    idList(a.scopeIds),
    a.lifecycleReader,
    a.actor.userId,
    from,
    to,
    to,
    person,
    person,
    a.actor.userId,
    q.mode === "mine",
    q.search,
    q.search,
    q.search,
    limit,
  );
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
        }).format(new Date())
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
  const cases = await rows<{
    id: string;
    kind: string;
    status: string;
    version: number;
    effective_at: number;
    last_day: string | null;
    departure_reason:string;
    timezone:string;
    person_id: string;
    person_name: string;
    branch_id: string | null;
    department_id: string | null;
    cutoff_at: number | null;
    directory_at: number | null;
    restored_at: number | null;
  }>(
    db,
    `SELECT c.id,c.kind,c.status,c.version,c.departure_reason,coalesce(c.definition->>'timezone','Asia/Bangkok') timezone,c.effective_at,c.definition->>'lastWorkingDate' last_day,c.person_id,p.legal_name person_name,e.branch_id,e.department_id,x.effective_at cutoff_at,x.restored_at,d.directory_at
 FROM people_lifecycle_cases c JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id JOIN hr_people p ON p.id=c.person_id AND p.company_id=c.company_id LEFT JOIN people_access_cutoffs x ON x.case_id=c.id AND x.company_id=c.company_id AND x.cancelled_at IS NULL
 LEFT JOIN LATERAL (SELECT max(o.updated_at) directory_at FROM people_directory_operations o WHERE o.company_id=c.company_id AND o.status='completed' AND o.result->>'verified'='true' AND o.definition->'action'->>'caseId'=c.id AND o.definition->'action'->>'kind'='account.disable') d ON true
 WHERE c.company_id=? AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")} AND c.status!='cancelled'
 AND (c.effective_at>=? AND c.effective_at<? OR c.definition->>'lastWorkingDate'>=? AND c.definition->>'lastWorkingDate'<? OR x.effective_at>=? AND x.effective_at<? OR d.directory_at>=? AND d.directory_at<?)
 AND (?::text IS NULL OR c.person_id=?) AND (?='' OR p.legal_name ILIKE '%'||?||'%') ORDER BY c.id LIMIT ?`,
    a.actor.companyId,
    idList(a.scopeIds),
    fromMs,
    toMs,
    from,
    to,
    fromMs,
    toMs,
    fromMs,
    toMs,
    person,
    person,
    q.search,
    q.search,
    limit,
  );
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
          c.effective_at > Date.now() &&
          c.effective_at < Date.now() + 7 * 86400000
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
          : c.cutoff_at <= Date.now()
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
