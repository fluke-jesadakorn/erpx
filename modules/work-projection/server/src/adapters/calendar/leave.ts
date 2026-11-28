import type { CalendarAccess } from "@erpx/work-projection-server/domain/calendar/access";
import "server-only";
import type { CalendarQuery } from "@erpx/work-projection-ui/lib/calendar";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { calendarLeavePeoplePayrollFacts } from "@erpx/people-payroll-server/public/calendar-leave-facts";
import { createCalendarLeave, type CalendarLeaveUnit } from "@erpx/work-projection-server/application/calendar/leave";
const service = createCalendarLeave();
export function calendarLeaveUnit(db: SqlDatabase): CalendarLeaveUnit {
    return { read1: facts => calendarLeavePeoplePayrollFacts(db).read1(facts),
        read2: facts => calendarLeavePeoplePayrollFacts(db).read2(facts) };
}
export function leaveItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.leaveItems(calendarLeaveUnit(db), a, q, from, to, limit); }
