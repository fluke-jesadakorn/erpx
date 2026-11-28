import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { calendarAvailabilityIdentityAccessFacts } from "@erpx/identity-access-server/public/calendar-availability-facts";
import { calendarAvailabilityWorkProjectionFacts } from "@erpx/work-projection-server/public/calendar-availability-facts";
import { calendarAvailabilityPeoplePayrollFacts } from "@erpx/people-payroll-server/public/calendar-availability-facts";
import { createCalendarAvailability, type CalendarAvailabilityUnit } from "@erpx/work-projection-server/application/calendar/availability";
import { bindLane29Context } from "../native-lane-29/client";
const service = createCalendarAvailability({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message) });
export function calendarAvailabilityUnit(db: SqlDatabase, actor?: WorkContext): CalendarAvailabilityUnit {
    if (actor) bindLane29Context(db, actor);
    return { read1: facts => calendarAvailabilityIdentityAccessFacts(db).read1(facts),
        read2: facts => calendarAvailabilityWorkProjectionFacts(db).read2(facts),
        read3: facts => calendarAvailabilityWorkProjectionFacts(db).read3(facts),
        read4: facts => calendarAvailabilityWorkProjectionFacts(db).read4(facts),
        read5: facts => calendarAvailabilityWorkProjectionFacts(db).read5(facts),
        read6: facts => calendarAvailabilityWorkProjectionFacts(db).read6(facts),
        read7: facts => calendarAvailabilityPeoplePayrollFacts(db).read7(facts) };
}
export function calendarAvailability(db: SqlDatabase, actor: WorkContext, ids: string[], from: number, to: number) { return service.calendarAvailability(calendarAvailabilityUnit(db, actor), actor, ids, from, to); }
