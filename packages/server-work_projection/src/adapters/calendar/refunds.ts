import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/feature-work_projection/lib/calendar";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { calendarRefundsFinanceFacts } from "@erpx/server-finance/public/calendar-refunds-facts";
import { createCalendarRefunds, type CalendarRefundsUnit } from "@erpx/server-work_projection/application/calendar/refunds";
const service = createCalendarRefunds();
export function calendarRefundsUnit(db: SqlDatabase,actor:CalendarAccess["actor"]): CalendarRefundsUnit { return { read1: facts => calendarRefundsFinanceFacts(db,actor).read1(facts) }; }
export function refundCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.refundCalendarItems(calendarRefundsUnit(db,a.actor), a, q, from, to, limit); }
