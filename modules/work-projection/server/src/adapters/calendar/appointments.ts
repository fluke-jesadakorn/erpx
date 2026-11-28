import { ApiError } from "@erpx/platform-runtime-server/public/http";
import type { CalendarAccess } from "@erpx/work-projection-server/domain/calendar/access";
import "server-only";
import type { CalendarQuery } from "@erpx/work-projection-ui/lib/calendar";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { runtimeEnv } from "../../../../../../platform/runtime/server/src/public/runtime";
import { calendarAppointmentsWorkProjectionFacts } from "@erpx/work-projection-server/public/calendar-appointments-facts";
import { calendarAppointmentsPeoplePayrollFacts } from "@erpx/people-payroll-server/public/calendar-appointments-facts";
import { calendarAppointmentsIdentityAccessFacts } from "@erpx/identity-access-server/public/calendar-appointments-facts";
import { createCalendarAppointments, type CalendarAppointmentsUnit } from "@erpx/work-projection-server/application/calendar/appointments";
import {calendarPurchaseControls,calendarPurchasedItems} from '@erpx/work-projection-server/adapters/calendar/purchases';
import { bindLane29Context } from "../native-lane-29/client";
const service = createCalendarAppointments({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message) });
export function calendarAppointmentsUnit(db: SqlDatabase, actor?: CalendarAccess['actor']): CalendarAppointmentsUnit {
    if (actor) bindLane29Context(db, actor);
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
export async function appointmentRows(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: number, to: number, limit: number) {const access=a.manualPurchase??await calendarPurchaseControls(db,a.actor);if(access&&!access.history)return [];return service.appointmentRows(calendarAppointmentsUnit(db, a.actor), a, q, from, to, limit); }
export async function appointmentItems(db: SqlDatabase, a: CalendarAccess, q: CalendarQuery, from: number, to: number, limit: number) {const access=a.manualPurchase??await calendarPurchaseControls(db,a.actor);if(access&&!access.history)return [];return calendarPurchasedItems(await service.appointmentItems(calendarAppointmentsUnit(db, a.actor), a, q, from, to, limit),access); }
export function appointmentWarningMessages(db: SqlDatabase, companyId: string, ids: string[], start: number, end: number, zone: string, except: string, userIds: string[] = []) { return service.appointmentWarningMessages(calendarAppointmentsUnit(db), companyId, ids, start, end, zone, except, userIds); }
export type { AppointmentRow } from "@erpx/work-projection-server/domain/calendar/appointments";
