import type { CalendarAccess } from "../../domain/calendar/access";
import { ApiError } from "../../../runtime/public/http";
import "server-only";
import type { CalendarQuery } from "@/lib/work_projection/calendar";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { calendarRosterPeoplePayrollFacts } from "../../../people_payroll/public/calendar-roster-facts";
import { createCalendarRoster, type CalendarRosterUnit } from "../../application/calendar/roster";
const service = createCalendarRoster({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message) });
export function calendarRosterUnit(db: SqlDatabase): CalendarRosterUnit {
    return { read1: facts => calendarRosterPeoplePayrollFacts(db).read1(facts),
        read2: facts => calendarRosterPeoplePayrollFacts(db).read2(facts) };
}
export function rosterItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: number, to: number, limit: number) { return service.rosterItems(calendarRosterUnit(db), a, q, from, to, limit); }
