import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/feature-work_projection/lib/calendar";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { calendarTransitInventoryFacts } from "@erpx/server-inventory/public/calendar-transit-facts";
import { createCalendarTransit, type CalendarTransitUnit } from "@erpx/server-work_projection/application/calendar/transit";
const service = createCalendarTransit();
export function calendarTransitUnit(db: SqlDatabase,a?:CalendarAccess): CalendarTransitUnit { return { read1: facts => calendarTransitInventoryFacts(db,a?.actor).read1(facts) }; }
export function transitCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.transitCalendarItems(calendarTransitUnit(db,a), a, q, from, to, limit); }
