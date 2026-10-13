import {hasPermission} from "@/lib/identity_access/permissions";
import type {WorkContext} from "../../../identity_access/public/authorization";
import type {StaffPerson} from "../../../operations/public/staff-policy";
import type {CalendarNode,CalendarPerson,CalendarHierarchy} from "@/server/modules/work_projection/public/rules/calendar-domain";
import {descendants,withinScope,type CalendarAccess} from "../../domain/calendar/access";
export type CalendarAccessUnit={activeStaff(actor:WorkContext):Promise<StaffPerson>;organizationNodes(companyId:string):Promise<CalendarNode[]>;permission(actor:WorkContext,permission:string):Promise<unknown>;people(input:{companyId:string;peopleIds:string[];lifecycleReader:boolean;search:string;take:number;cursor:number}):Promise<CalendarPerson[]>};
export function createCalendarAccess(ports:{errorCode(error:unknown):string|undefined;fail(message:string,code?:string,status?:number):never}){
async function calendarAccess(
  db: CalendarAccessUnit,
  actor: WorkContext,
): Promise<CalendarAccess> {
  if (actor.beta) ports.fail("Calendar is unavailable in this workspace.");
  let staff: StaffPerson | null = null;
  try {
    staff = await db.activeStaff(actor);
  } catch (e) {
    if (ports.errorCode(e)!=="STAFF_LINK_REQUIRED") throw e;
  }
  const nodes = await db.organizationNodes(actor.companyId);
  if (nodes.length > 5000)
    ports.fail(
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
function requirePlacement(
  access: CalendarAccess,
  branch: string | null,
  department: string | null,
) {
  if (!withinScope(access, branch, department))
    ports.fail("This record is outside the selected workspace.");
}
async function needPermission(
  db: CalendarAccessUnit,
  actor: WorkContext,
  permission: string,
) {
  if (
    !hasPermission(actor, permission) ||
    !(await db.permission(actor,permission))
  )
    ports.fail(
      "Your selected role cannot perform this action.",
      "PERMISSION_REQUIRED",
    );
}
async function calendarHierarchy(
  db: CalendarAccessUnit,
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
    ports.fail("Invalid people page.", "VALIDATION_FAILED", 422);
  const people = await db.people({companyId:actor.companyId,peopleIds:access.peopleIds,lifecycleReader:access.lifecycleReader,search:query.search??"",take:limit+1,cursor});
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

return {calendarAccess,requirePlacement,needPermission,calendarHierarchy};}
