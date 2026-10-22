import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
import { ApiError } from "@erpx/server-runtime/public/http";
import "server-only";
import type { CalendarQuery } from "@erpx/feature-work_projection/lib/calendar";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { calendarLifecyclePeoplePayrollFacts } from "@erpx/server-people_payroll/public/calendar-lifecycle-facts";
import { createCalendarLifecycle, type CalendarLifecycleUnit } from "@erpx/server-work_projection/application/calendar/lifecycle";
const service = createCalendarLifecycle({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message) });
export function calendarLifecycleUnit(db: SqlDatabase): CalendarLifecycleUnit {
    return { read1: facts => calendarLifecyclePeoplePayrollFacts(db).read1(facts),
        read2: facts => calendarLifecyclePeoplePayrollFacts(db).read2(facts) };
}
export function lifecycleItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, fromMs: number, toMs: number, limit: number) { return service.lifecycleItems(calendarLifecycleUnit(db), a, q, from, to, fromMs, toMs, limit); }
