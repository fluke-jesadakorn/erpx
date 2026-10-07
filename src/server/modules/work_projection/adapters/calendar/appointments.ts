import { ApiError } from "../../../runtime/public/http";
import type { CalendarAccess } from "../../domain/calendar/access";
import "server-only";
import type { CalendarQuery } from "@/lib/calendar";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { calendarAppointmentsWorkProjectionFacts } from "../../../work_projection/public/calendar-appointments-facts";
import { calendarAppointmentsPeoplePayrollFacts } from "../../../people_payroll/public/calendar-appointments-facts";
import { calendarAppointmentsIdentityAccessFacts } from "../../../identity_access/public/calendar-appointments-facts";
import { createCalendarAppointments, type CalendarAppointmentsUnit } from "../../application/calendar/appointments";
const service = createCalendarAppointments({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message) });
export function calendarAppointmentsUnit(db: SqlDatabase): CalendarAppointmentsUnit {
    return { read1: facts => calendarAppointmentsWorkProjectionFacts(db).read1(facts),
        read2: facts => calendarAppointmentsWorkProjectionFacts(db).read2(facts),
        read3: facts => calendarAppointmentsWorkProjectionFacts(db).read3(facts),
        read4: facts => calendarAppointmentsWorkProjectionFacts(db).read4(facts),
        read5: facts => calendarAppointmentsWorkProjectionFacts(db).read5(facts),
        read6: facts => calendarAppointmentsWorkProjectionFacts(db).read6(facts),
        read7: facts => calendarAppointmentsPeoplePayrollFacts(db).read7(facts),
        read8: facts => calendarAppointmentsWorkProjectionFacts(db).read8(facts),
        read9: async (facts) => calendarAppointmentsIdentityAccessFacts((await runtimeEnv()).CONTROL_DB).read9(facts) };
}
export function appointmentRows(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: number, to: number, limit: number) { return service.appointmentRows(calendarAppointmentsUnit(db), a, q, from, to, limit); }
export function appointmentItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: number, to: number, limit: number) { return service.appointmentItems(calendarAppointmentsUnit(db), a, q, from, to, limit); }
export function appointmentWarningMessages(db: SqlDatabase, companyId: string, ids: string[], start: number, end: number, zone: string, except: string, userIds: string[] = []) { return service.appointmentWarningMessages(calendarAppointmentsUnit(db), companyId, ids, start, end, zone, except, userIds); }
export type { AppointmentRow } from "../../domain/calendar/appointments";
