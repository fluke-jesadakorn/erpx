import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type { CalendarAccess } from "@erpx/work-projection-server/domain/calendar/access";
import 'server-only';
import type { CalendarQuery } from "@erpx/work-projection-ui/lib/calendar";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { calendarCustomerFollowupsSalesFacts } from "@erpx/sales-server/public/calendar-customer-followups-facts";
import { createCalendarCustomerFollowups, type CalendarCustomerFollowupsUnit } from "@erpx/work-projection-server/application/calendar/customer-followups";
const service = createCalendarCustomerFollowups();
export function calendarCustomerFollowupsUnit(db: SqlDatabase,actor:WorkContext): CalendarCustomerFollowupsUnit { return { read1: facts => calendarCustomerFollowupsSalesFacts(db,actor).read1(facts) }; }
export function customerFollowupCalendarItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.customerFollowupCalendarItems(calendarCustomerFollowupsUnit(db,a.actor), a, q, from, to, limit); }
