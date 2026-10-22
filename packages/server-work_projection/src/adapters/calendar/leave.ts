import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
import "server-only";
import type { CalendarQuery } from "@erpx/feature-work_projection/lib/calendar";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { calendarLeavePeoplePayrollFacts } from "@erpx/server-people_payroll/public/calendar-leave-facts";
import { createCalendarLeave, type CalendarLeaveUnit } from "@erpx/server-work_projection/application/calendar/leave";
const service = createCalendarLeave();
export function calendarLeaveUnit(db: SqlDatabase): CalendarLeaveUnit {
    return { read1: facts => calendarLeavePeoplePayrollFacts(db).read1(facts),
        read2: facts => calendarLeavePeoplePayrollFacts(db).read2(facts) };
}
export function leaveItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, limit: number) { return service.leaveItems(calendarLeaveUnit(db), a, q, from, to, limit); }
