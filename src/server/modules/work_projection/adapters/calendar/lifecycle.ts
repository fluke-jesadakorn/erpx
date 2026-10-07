import type { CalendarAccess } from "../../domain/calendar/access";
import { ApiError } from "../../../runtime/public/http";
import "server-only";
import type { CalendarQuery } from "@/lib/calendar";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { calendarLifecyclePeoplePayrollFacts } from "../../../people_payroll/public/calendar-lifecycle-facts";
import { createCalendarLifecycle, type CalendarLifecycleUnit } from "../../application/calendar/lifecycle";
const service = createCalendarLifecycle({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message) });
export function calendarLifecycleUnit(db: SqlDatabase): CalendarLifecycleUnit {
    return { read1: facts => calendarLifecyclePeoplePayrollFacts(db).read1(facts),
        read2: facts => calendarLifecyclePeoplePayrollFacts(db).read2(facts) };
}
export function lifecycleItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: string, to: string, fromMs: number, toMs: number, limit: number) { return service.lifecycleItems(calendarLifecycleUnit(db), a, q, from, to, fromMs, toMs, limit); }
