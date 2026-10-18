import type { CalendarAccess } from "../../domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@/lib/work_projection/calendar";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { calendarProcurementRfqsProcurementFacts } from "../../../procurement/public/calendar-procurement-rfqs-facts";
import { createCalendarProcurementRfqs, type CalendarProcurementRfqsUnit } from "../../application/calendar/procurement-rfqs";
const service = createCalendarProcurementRfqs();
export function calendarProcurementRfqsUnit(db: SqlDatabase,actor?:CalendarAccess["actor"]): CalendarProcurementRfqsUnit { return { read1: facts => calendarProcurementRfqsProcurementFacts(db,actor).read1(facts) }; }
export function rfqCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.rfqCalendarItems(calendarProcurementRfqsUnit(db,a.actor), a, q, from, to, limit); }
