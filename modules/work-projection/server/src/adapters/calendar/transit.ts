import type { CalendarAccess } from "@erpx/work-projection-server/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/work-projection-ui/lib/calendar";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { calendarTransitInventoryFacts } from "@erpx/inventory-server/public/calendar-transit-facts";
import { createCalendarTransit, type CalendarTransitUnit } from "@erpx/work-projection-server/application/calendar/transit";
const service = createCalendarTransit();
export function calendarTransitUnit(db: SqlDatabase,a?:CalendarAccess): CalendarTransitUnit { return { read1: facts => calendarTransitInventoryFacts(db,a?.actor).read1(facts) }; }
export function transitCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.transitCalendarItems(calendarTransitUnit(db,a), a, q, from, to, limit); }
