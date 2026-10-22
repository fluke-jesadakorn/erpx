import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/feature-work_projection/lib/calendar";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { calendarCustomerFollowupsSalesFacts } from "@erpx/server-sales/public/calendar-customer-followups-facts";
import { createCalendarCustomerFollowups, type CalendarCustomerFollowupsUnit } from "@erpx/server-work_projection/application/calendar/customer-followups";
const service = createCalendarCustomerFollowups();
export function calendarCustomerFollowupsUnit(db: SqlDatabase,actor:WorkContext): CalendarCustomerFollowupsUnit { return { read1: facts => calendarCustomerFollowupsSalesFacts(db,actor).read1(facts) }; }
export function customerFollowupCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.customerFollowupCalendarItems(calendarCustomerFollowupsUnit(db,a.actor), a, q, from, to, limit); }
