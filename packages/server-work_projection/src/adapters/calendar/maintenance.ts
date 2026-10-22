import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
import 'server-only';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { CalendarQuery } from "@erpx/feature-work_projection/lib/calendar";
import { calendarMaintenanceMaintenanceFacts } from "@erpx/server-maintenance/public/calendar-maintenance-facts";
import { createCalendarMaintenance, type CalendarMaintenanceUnit } from "@erpx/server-work_projection/application/calendar/maintenance";
const service = createCalendarMaintenance();
export function calendarMaintenanceUnit(db: SqlDatabase,actor?:WorkContext): CalendarMaintenanceUnit { return { read1: facts => calendarMaintenanceMaintenanceFacts(db,actor).read1(facts) }; }
export function maintenanceCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: number, to: number, limit: number) { return service.maintenanceCalendarItems(calendarMaintenanceUnit(db,a.actor), a, q, from, to, limit); }
