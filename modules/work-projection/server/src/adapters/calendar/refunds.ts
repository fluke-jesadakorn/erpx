import type { CalendarAccess } from "@erpx/work-projection-server/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/work-projection-ui/lib/calendar";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { calendarRefundsFinanceFacts } from "@erpx/finance-server/public/calendar-refunds-facts";
import { createCalendarRefunds, type CalendarRefundsUnit } from "@erpx/work-projection-server/application/calendar/refunds";
const service = createCalendarRefunds();
export function calendarRefundsUnit(db: SqlDatabase,actor:CalendarAccess["actor"]): CalendarRefundsUnit { return { read1: facts => calendarRefundsFinanceFacts(db,actor).read1(facts) }; }
export function refundCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.refundCalendarItems(calendarRefundsUnit(db,a.actor), a, q, from, to, limit); }
