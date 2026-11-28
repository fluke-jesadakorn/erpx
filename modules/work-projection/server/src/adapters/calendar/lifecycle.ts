import type { CalendarAccess } from "@erpx/work-projection-server/domain/calendar/access";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import "server-only";
import type { CalendarQuery } from "@erpx/work-projection-ui/lib/calendar";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { calendarLifecyclePeoplePayrollFacts } from "@erpx/people-payroll-server/public/calendar-lifecycle-facts";
import { createCalendarLifecycle, type CalendarLifecycleUnit } from "@erpx/work-projection-server/application/calendar/lifecycle";
const service = createCalendarLifecycle({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message) });
export function calendarLifecycleUnit(db: SqlDatabase): CalendarLifecycleUnit {
    return { read1: facts => calendarLifecyclePeoplePayrollFacts(db).read1(facts),
        read2: facts => calendarLifecyclePeoplePayrollFacts(db).read2(facts) };
}
export function lifecycleItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, fromMs: number, toMs: number, limit: number) { return service.lifecycleItems(calendarLifecycleUnit(db), a, q, from, to, fromMs, toMs, limit); }
