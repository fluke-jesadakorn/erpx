import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/feature-work_projection/lib/calendar";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { calendarCrmCrmFacts } from "@erpx/server-crm/public/calendar-crm-facts";
import { createCalendarCrm, type CalendarCrmUnit } from "@erpx/server-work_projection/application/calendar/crm";
const service = createCalendarCrm();
export function calendarCrmUnit(db: SqlDatabase,actor:WorkContext): CalendarCrmUnit { return { read1: facts => calendarCrmCrmFacts(db,actor).read1(facts) }; }
export function crmCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.crmCalendarItems(calendarCrmUnit(db,a.actor), a, q, from, to, limit); }
