import { ApiError } from "@erpx/server-runtime/public/http";
import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
import "server-only";
import type { CalendarQuery } from "@erpx/feature-work_projection/lib/calendar";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { runtimeEnv } from "../../../../../packages/server-runtime/src/public/runtime";
import { calendarAppointmentsWorkProjectionFacts } from "@erpx/server-work_projection/public/calendar-appointments-facts";
import { calendarAppointmentsPeoplePayrollFacts } from "@erpx/server-people_payroll/public/calendar-appointments-facts";
import { calendarAppointmentsIdentityAccessFacts } from "@erpx/server-identity_access/public/calendar-appointments-facts";
import { createCalendarAppointments, type CalendarAppointmentsUnit } from "@erpx/server-work_projection/application/calendar/appointments";
import {calendarPurchaseControls,calendarPurchasedItems} from '@erpx/server-work_projection/adapters/calendar/purchases';
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
export async function appointmentRows(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: number, to: number, limit: number) {const access=a.manualPurchase??await calendarPurchaseControls(db,a.actor);if(access&&!access.history)return [];return service.appointmentRows(calendarAppointmentsUnit(db), a, q, from, to, limit); }
export async function appointmentItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: number, to: number, limit: number) {const access=a.manualPurchase??await calendarPurchaseControls(db,a.actor);if(access&&!access.history)return [];return calendarPurchasedItems(await service.appointmentItems(calendarAppointmentsUnit(db), a, q, from, to, limit),access); }
export function appointmentWarningMessages(db: SqlDatabase, companyId: string, ids: string[], start: number, end: number, zone: string, except: string, userIds: string[] = []) { return service.appointmentWarningMessages(calendarAppointmentsUnit(db), companyId, ids, start, end, zone, except, userIds); }
export type { AppointmentRow } from "@erpx/server-work_projection/domain/calendar/appointments";
