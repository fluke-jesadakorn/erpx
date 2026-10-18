import type { CalendarAccess } from "../../domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@/lib/work_projection/calendar";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { calendarTransitInventoryFacts } from "../../../inventory/public/calendar-transit-facts";
import { createCalendarTransit, type CalendarTransitUnit } from "../../application/calendar/transit";
const service = createCalendarTransit();
export function calendarTransitUnit(db: SqlDatabase,a?:CalendarAccess): CalendarTransitUnit { return { read1: facts => calendarTransitInventoryFacts(db,a?.actor).read1(facts) }; }
export function transitCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.transitCalendarItems(calendarTransitUnit(db,a), a, q, from, to, limit); }
