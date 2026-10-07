import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError } from "@/server/modules/runtime/public/http";
import { calendarAvailabilityIdentityAccessFacts } from "../../../identity_access/public/calendar-availability-facts";
import { calendarAvailabilityWorkProjectionFacts } from "../../../work_projection/public/calendar-availability-facts";
import { calendarAvailabilityPeoplePayrollFacts } from "../../../people_payroll/public/calendar-availability-facts";
import { createCalendarAvailability, type CalendarAvailabilityUnit } from "../../application/calendar/availability";
const service = createCalendarAvailability({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message) });
export function calendarAvailabilityUnit(db: SqlDatabase): CalendarAvailabilityUnit {
    return { read1: facts => calendarAvailabilityIdentityAccessFacts(db).read1(facts),
        read2: facts => calendarAvailabilityWorkProjectionFacts(db).read2(facts),
        read3: facts => calendarAvailabilityWorkProjectionFacts(db).read3(facts),
        read4: facts => calendarAvailabilityWorkProjectionFacts(db).read4(facts),
        read5: facts => calendarAvailabilityWorkProjectionFacts(db).read5(facts),
        read6: facts => calendarAvailabilityWorkProjectionFacts(db).read6(facts),
        read7: facts => calendarAvailabilityPeoplePayrollFacts(db).read7(facts) };
}
export function calendarAvailability(db: SqlDatabase, actor: WorkContext, ids: string[], from: number, to: number) { return service.calendarAvailability(calendarAvailabilityUnit(db), actor, ids, from, to); }
