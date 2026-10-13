import type { CalendarAccess } from "../../domain/calendar/access";
import 'server-only';
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { CalendarQuery } from "@/lib/work_projection/calendar";
import { calendarMaintenanceMaintenanceFacts } from "../../../maintenance/public/calendar-maintenance-facts";
import { createCalendarMaintenance, type CalendarMaintenanceUnit } from "../../application/calendar/maintenance";
const service = createCalendarMaintenance();
export function calendarMaintenanceUnit(db: SqlDatabase): CalendarMaintenanceUnit { return { read1: facts => calendarMaintenanceMaintenanceFacts(db).read1(facts) }; }
export function maintenanceCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: number, to: number, limit: number) { return service.maintenanceCalendarItems(calendarMaintenanceUnit(db), a, q, from, to, limit); }
