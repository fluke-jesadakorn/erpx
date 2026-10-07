import type { CalendarAccess } from "../../domain/calendar/access";
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { activeStaff } from "@/server/modules/operations/public/staff-policy";
import { ApiError } from "@/server/modules/runtime/public/http";
export const rows = async <T>(db: SqlDatabase, sql: string, ...args: (string | number | null | boolean)[]) => (await db
    .prepare(sql)
    .bind(...args)
    .all<T>()).results;
export const one = <T>(db: SqlDatabase, sql: string, ...args: (string | number | null | boolean)[]) => db
    .prepare(sql)
    .bind(...args)
    .first<T>();
export const run = (db: SqlDatabase, sql: string, ...args: (string | number | null | boolean)[]) => db
    .prepare(sql)
    .bind(...args)
    .run();
export function fail(message: string, code = "CALENDAR_FORBIDDEN", status = 403): never {
    throw new ApiError(status, code, message);
}
import { createCalendarAccess, type CalendarAccessUnit } from "../../application/calendar/access";
import { descendants, withinScope } from "../../domain/calendar/access";
export { descendants, withinScope };
export type { CalendarAccess } from "../../domain/calendar/access";
import { calendarAccessOrganizationFacts } from "../../../organization/public/calendar-access-facts";
import { calendarAccessIdentityAccessFacts } from "../../../identity_access/public/calendar-access-facts";
import { calendarAccessPeoplePayrollFacts } from "../../../people_payroll/public/calendar-access-facts";
export const idList = (ids: string[]) => JSON.stringify(ids);
export const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarAccessUnit(db: SqlDatabase): CalendarAccessUnit { return { activeStaff: actor => activeStaff(db, actor.companyId, actor.userId), organizationNodes: companyId => calendarAccessOrganizationFacts(db).organizationNodes(companyId), permission: (actor, permission) => calendarAccessIdentityAccessFacts(db).permission(actor, permission), people: input => calendarAccessPeoplePayrollFacts(db).people(input) }; }
const service = createCalendarAccess({ errorCode: error => error instanceof ApiError ? error.code : undefined, fail });
export function calendarAccess(db: SqlDatabase, actor: WorkContext) { return service.calendarAccess(calendarAccessUnit(db), actor); }
export function requirePlacement(access: CalendarAccess, branch: string | null, department: string | null) { return service.requirePlacement(access, branch, department); }
export function needPermission(db: SqlDatabase, actor: WorkContext, permission: string) { return service.needPermission(calendarAccessUnit(db), actor, permission); }
export function calendarHierarchy(db: SqlDatabase, actor: WorkContext, query: {
    cursor?: number;
    search?: string;
    limit?: number;
} = {}) { return service.calendarHierarchy(calendarAccessUnit(db), actor, query); }
