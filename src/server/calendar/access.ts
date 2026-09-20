import "server-only";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { hasPermission } from "@/lib/permissions";
import type {
  CalendarNode,
  CalendarPerson,
  CalendarHierarchy,
} from "@/lib/calendar";
import { activeStaff, type StaffPerson } from "../staff-policy";
import { ApiError } from "../http";
export const rows = async <T>(
  db: SqlDatabase,
  sql: string,
  ...args: (string | number | null | boolean)[]
) =>
  (
    await db
      .prepare(sql)
      .bind(...args)
      .all<T>()
  ).results;
export const one = <T>(
  db: SqlDatabase,
  sql: string,
  ...args: (string | number | null | boolean)[]
) =>
  db
    .prepare(sql)
    .bind(...args)
    .first<T>();
export const run = (
  db: SqlDatabase,
  sql: string,
  ...args: (string | number | null | boolean)[]
) =>
  db
    .prepare(sql)
    .bind(...args)
    .run();
export function fail(
  message: string,
  code = "CALENDAR_FORBIDDEN",
  status = 403,
): never {
  throw new ApiError(status, code, message);
}
export type CalendarAccess = {
  actor: WorkContext;
  staff: StaffPerson | null;
  nodes: CalendarNode[];
  scopeIds: string[];
  rosterIds: string[];
  peopleIds: string[];
  availability: boolean;
  rosterReader: boolean;
  lifecycleReader: boolean;
  teamDefault: boolean;
};
export const idList = (ids: string[]) => JSON.stringify(ids);
export const inList = (expression: string) =>
  `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function descendants(nodes: CalendarNode[], root: string) {
  const found = new Set<string>([root]);
  for (let i = 0; i < nodes.length; i++) {
    const before = found.size;
    for (const n of nodes)
      if (n.parentId && found.has(n.parentId)) found.add(n.id);
    if (before === found.size) break;
  }
  return [...found];
}
export async function calendarAccess(
  db: SqlDatabase,
  actor: WorkContext,
): Promise<CalendarAccess> {
  if (actor.beta) fail("Calendar is unavailable in this workspace.");
  let staff: StaffPerson | null = null;
  try {
    staff = await activeStaff(db, actor.companyId, actor.userId);
  } catch (e) {
    if (!(e instanceof ApiError) || e.code !== "STAFF_LINK_REQUIRED") throw e;
  }
  const nodes = await rows<CalendarNode>(
    db,
    'SELECT id,parent_id "parentId",display_name name,kind,branch_id "branchId" FROM organization_nodes WHERE company_id=? ORDER BY id LIMIT 5001',
    actor.companyId,
  );
  if (nodes.length > 5000)
    fail(
      "This organization exceeds the calendar navigation limit.",
      "CALENDAR_LIMIT",
      422,
    );
  const root = actor.departmentId ?? actor.branchId ?? actor.companyId;
  const scopeIds = descendants(nodes, root);
  const rosterReader = ["roster.read", "roster.manage", "roster.approve"].some(
    (p) => hasPermission(actor, p),
  );
  const availability = hasPermission(actor, "calendar.availability.read");
  const lifecycleReader = ["people.manage", "iam.directory.manage"].some((p) =>
    hasPermission(actor, p),
  );
  const own =
    staff?.department_id && scopeIds.includes(staff.department_id)
      ? [staff.department_id]
      : [];
  return {
    actor,
    staff,
    nodes,
    scopeIds,
    rosterIds: rosterReader ? scopeIds : own,
    peopleIds:
      availability ||
      rosterReader ||
      lifecycleReader ||
      hasPermission(actor, "calendar.appointments.manage")
        ? scopeIds
        : own,
    availability,
    rosterReader,
    lifecycleReader,
    teamDefault: rosterReader || lifecycleReader || availability,
  };
}
export function withinScope(
  access: CalendarAccess,
  branch: string | null,
  department: string | null,
) {
  return access.scopeIds.includes(
    department ?? branch ?? access.actor.companyId,
  );
}
export function requirePlacement(
  access: CalendarAccess,
  branch: string | null,
  department: string | null,
) {
  if (!withinScope(access, branch, department))
    fail("This record is outside the selected workspace.");
}
export async function needPermission(
  db: SqlDatabase,
  actor: WorkContext,
  permission: string,
) {
  if (
    !hasPermission(actor, permission) ||
    !(await one(
      db,
      "SELECT 1 WHERE iam_role_allows(?,?,?)",
      actor.role,
      actor.companyId,
      permission,
    ))
  )
    fail(
      "Your selected role cannot perform this action.",
      "PERMISSION_REQUIRED",
    );
}
export async function calendarHierarchy(
  db: SqlDatabase,
  actor: WorkContext,
  query: { cursor?: number; search?: string; limit?: number } = {},
): Promise<CalendarHierarchy> {
  const access = await calendarAccess(db, actor),
    cursor = query.cursor ?? 0,
    limit = query.limit ?? 200;
  if (
    !Number.isInteger(cursor) ||
    cursor < 0 ||
    cursor > 10000 ||
    !Number.isInteger(limit) ||
    limit < 1 ||
    limit > 200
  )
    fail("Invalid people page.", "VALIDATION_FAILED", 422);
  const people = await rows<CalendarPerson>(
    db,
    `SELECT * FROM (SELECT DISTINCT ON(p.id) p.id,p.legal_name name,p.user_id "userId",e.department_id "departmentId",e.branch_id "branchId",e.manager_person_id "managerId",e.status
  FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
  WHERE p.company_id=? AND p.merged_into_id IS NULL AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")}
  AND ((? AND (e.status!='ended' OR EXISTS(SELECT 1 FROM people_lifecycle_cases lc WHERE lc.engagement_id=e.id AND lc.company_id=e.company_id))) OR (e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))) AND (?='' OR p.legal_name ILIKE '%'||?||'%')
  ORDER BY p.id,CASE WHEN e.status='active' THEN 0 ELSE 1 END,e.start_date DESC,e.id) x ORDER BY name,id LIMIT ? OFFSET ?`,
    actor.companyId,
    idList(access.peopleIds),
    access.lifecycleReader,
    query.search ?? "",
    query.search ?? "",
    limit + 1,
    cursor,
  );
  const visible = new Set(access.peopleIds);
  for (const n of access.nodes)
    if (visible.has(n.id)) {
      let parent = n.parentId;
      const seen = new Set<string>();
      while (parent && !seen.has(parent)) {
        seen.add(parent);
        visible.add(parent);
        parent = access.nodes.find((x) => x.id === parent)?.parentId ?? null;
      }
    }
  return {
    nodes: access.nodes.filter((n) => visible.has(n.id)).map(n=>({...n,inScope:access.scopeIds.includes(n.id)})),
    people: people.slice(0, limit),
    nextCursor: people.length > limit ? cursor + limit : null,
    selfPersonId: access.staff?.person_id ?? null,
  };
}
