import type { CalendarAccess } from "../../domain/calendar/access";
import { ApiError } from "../../../runtime/public/http";
import "server-only";
import type { CalendarQuery } from "@/lib/work_projection/calendar";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { calendarRosterPeoplePayrollFacts } from "../../../people_payroll/public/calendar-roster-facts";
import { createCalendarRoster, type CalendarRosterUnit } from "../../application/calendar/roster";
import { rosterSourceAccess } from "../../../people_payroll/public/roster-purchases";
const service = createCalendarRoster({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message) });
export function calendarRosterUnit(db: SqlDatabase): CalendarRosterUnit {
    return { read1: facts => calendarRosterPeoplePayrollFacts(db).read1(facts),
        read2: facts => calendarRosterPeoplePayrollFacts(db).read2(facts) };
}
export async function rosterItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: number, to: number, limit: number) {
    const purchase = await rosterSourceAccess(db, a.actor);
    if (purchase && !purchase.history) return [];
    const items = await service.rosterItems(calendarRosterUnit(db), a, q, from, to, limit);
    return purchase ? items.map(item => ({...item, permittedActions: item.permittedActions.filter(action =>
        action !== "roster.manage" && action !== "shift.reschedule" || (action === "roster.manage" ? purchase.newWork : purchase.settleExisting))})) : items;
}
