import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type { CalendarAccess } from "@erpx/work-projection-server/domain/calendar/access";
import 'server-only';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { CalendarQuery } from "@erpx/work-projection-ui/lib/calendar";
import { calendarMaintenanceMaintenanceFacts } from "@erpx/maintenance-server/public/calendar-maintenance-facts";
import { createCalendarMaintenance, type CalendarMaintenanceUnit } from "@erpx/work-projection-server/application/calendar/maintenance";
const service = createCalendarMaintenance();
export function calendarMaintenanceUnit(db: SqlDatabase,actor?:WorkContext): CalendarMaintenanceUnit { return { read1: facts => calendarMaintenanceMaintenanceFacts(db,actor).read1(facts) }; }
export function maintenanceCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: number, to: number, limit: number) { return service.maintenanceCalendarItems(calendarMaintenanceUnit(db,a.actor), a, q, from, to, limit); }
