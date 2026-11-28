import type { CalendarAccess } from "@erpx/work-projection-server/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/work-projection-ui/lib/calendar";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { calendarProcurementRfqsProcurementFacts } from "@erpx/procurement-server/public/calendar-procurement-rfqs-facts";
import { createCalendarProcurementRfqs, type CalendarProcurementRfqsUnit } from "@erpx/work-projection-server/application/calendar/procurement-rfqs";
const service = createCalendarProcurementRfqs();
export function calendarProcurementRfqsUnit(db: SqlDatabase,actor?:CalendarAccess["actor"]): CalendarProcurementRfqsUnit { return { read1: facts => calendarProcurementRfqsProcurementFacts(db,actor).read1(facts) }; }
export function rfqCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.rfqCalendarItems(calendarProcurementRfqsUnit(db,a.actor), a, q, from, to, limit); }
