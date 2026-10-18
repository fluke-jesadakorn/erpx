import type { CalendarAccess } from "../../domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@/lib/work_projection/calendar";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { calendarRefundsFinanceFacts } from "../../../finance/public/calendar-refunds-facts";
import { createCalendarRefunds, type CalendarRefundsUnit } from "../../application/calendar/refunds";
const service = createCalendarRefunds();
export function calendarRefundsUnit(db: SqlDatabase,actor:CalendarAccess["actor"]): CalendarRefundsUnit { return { read1: facts => calendarRefundsFinanceFacts(db,actor).read1(facts) }; }
export function refundCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.refundCalendarItems(calendarRefundsUnit(db,a.actor), a, q, from, to, limit); }
