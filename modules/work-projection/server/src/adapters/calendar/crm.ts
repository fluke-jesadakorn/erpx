import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type { CalendarAccess } from "@erpx/work-projection-server/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/work-projection-ui/lib/calendar";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { calendarCrmCrmFacts } from "@erpx/crm-server/public/calendar-crm-facts";
import { createCalendarCrm, type CalendarCrmUnit } from "@erpx/work-projection-server/application/calendar/crm";
const service = createCalendarCrm();
export function calendarCrmUnit(db: SqlDatabase,actor:WorkContext): CalendarCrmUnit { return { read1: facts => calendarCrmCrmFacts(db,actor).read1(facts) }; }
export function crmCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.crmCalendarItems(calendarCrmUnit(db,a.actor), a, q, from, to, limit); }
