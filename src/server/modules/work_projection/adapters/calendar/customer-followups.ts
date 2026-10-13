import type { CalendarAccess } from "../../domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@/lib/work_projection/calendar";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { calendarCustomerFollowupsSalesFacts } from "../../../sales/public/calendar-customer-followups-facts";
import { createCalendarCustomerFollowups, type CalendarCustomerFollowupsUnit } from "../../application/calendar/customer-followups";
const service = createCalendarCustomerFollowups();
export function calendarCustomerFollowupsUnit(db: SqlDatabase): CalendarCustomerFollowupsUnit { return { read1: facts => calendarCustomerFollowupsSalesFacts(db).read1(facts) }; }
export function customerFollowupCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.customerFollowupCalendarItems(calendarCustomerFollowupsUnit(db), a, q, from, to, limit); }
