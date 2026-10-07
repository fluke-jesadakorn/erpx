import { requireClearanceCompletion, requireClearancePlan } from "@/server/modules/people_payroll/public/retirement-clearance";
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { rescheduleRosterShift } from "@/server/modules/people_payroll/public/people-roster";
import { publishInboxEvent } from "@/server/modules/collaboration/public/inbox";
import { prepareAttachmentBinding } from "@/server/modules/files/public/attachments/service";
import type { SqlStatement } from "../../../runtime/public/runtime/types";
import type { Result } from "../../domain/calendar/commands";
import { calendarAccess, needPermission, requirePlacement, fail } from "./access";
import { appointmentWarningMessages } from "./appointments";
import { calendarCommandsWorkProjectionFacts } from "../../../work_projection/public/calendar-commands-facts";
import { calendarCommandsOperationsFacts } from "../../../operations/public/calendar-commands-facts";
import { calendarCommandsPeoplePayrollFacts } from "../../../people_payroll/public/calendar-commands-facts";
import { calendarCommandsIdentityAccessFacts } from "../../../identity_access/public/calendar-commands-facts";
import { createCalendarCommands, type CalendarCommandsUnit } from "../../application/calendar/commands";
const service = createCalendarCommands<SqlStatement>({ clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, fail, requirePlacement });
export function calendarCommandsUnit(db: SqlDatabase): CalendarCommandsUnit<SqlStatement> {
    return { reschedule: (...args) => rescheduleRosterShift(db, ...args), read1: facts => calendarCommandsWorkProjectionFacts(db).read1(facts),
        read2: facts => calendarCommandsOperationsFacts(db).read2(facts),
        read3: facts => calendarCommandsWorkProjectionFacts(db).read3(facts),
        read4: facts => calendarCommandsWorkProjectionFacts(db).read4(facts),
        read5: facts => calendarCommandsPeoplePayrollFacts(db).read5(facts),
        read6: facts => calendarCommandsIdentityAccessFacts(db).read6(facts),
        read7: facts => calendarCommandsPeoplePayrollFacts(db).read7(facts),
        write8: facts => calendarCommandsPeoplePayrollFacts(db).write8(facts),
        write9: facts => calendarCommandsPeoplePayrollFacts(db).write9(facts),
        read10: facts => calendarCommandsIdentityAccessFacts(db).read10(facts),
        write11: facts => calendarCommandsPeoplePayrollFacts(db).write11(facts),
        read12: facts => calendarCommandsWorkProjectionFacts(db).read12(facts),
        read13: facts => calendarCommandsWorkProjectionFacts(db).read13(facts),
        write14: facts => calendarCommandsWorkProjectionFacts(db).write14(facts),
        write15: facts => calendarCommandsWorkProjectionFacts(db).write15(facts),
        write16: facts => calendarCommandsWorkProjectionFacts(db).write16(facts),
        write18: facts => calendarCommandsWorkProjectionFacts(db).write18(facts),
        write19: facts => calendarCommandsWorkProjectionFacts(db).write19(facts),
        write20: facts => calendarCommandsWorkProjectionFacts(db).write20(facts),
        write21: facts => calendarCommandsWorkProjectionFacts(db).write21(facts),
        write22: facts => calendarCommandsWorkProjectionFacts(db).write22(facts),
        read23: facts => calendarCommandsWorkProjectionFacts(db).read23(facts),
        write24: facts => calendarCommandsWorkProjectionFacts(db).write24(facts),
        read25: facts => calendarCommandsPeoplePayrollFacts(db).read25(facts),
        read26: facts => calendarCommandsWorkProjectionFacts(db).read26(facts),
        read27: facts => calendarCommandsPeoplePayrollFacts(db).read27(facts),
        read28: facts => calendarCommandsIdentityAccessFacts(db).read28(facts),
        write29: facts => calendarCommandsWorkProjectionFacts(db).write29(facts),
        read30: facts => calendarCommandsWorkProjectionFacts(db).read30(facts),
        write31: facts => calendarCommandsWorkProjectionFacts(db).write31(facts),
        write32: facts => calendarCommandsWorkProjectionFacts(db).write32(facts),
        write33: facts => calendarCommandsWorkProjectionFacts(db).write33(facts),
        write34: facts => calendarCommandsWorkProjectionFacts(db).write34(facts),
        write35: facts => calendarCommandsWorkProjectionFacts(db).write35(facts),
        write36: facts => calendarCommandsWorkProjectionFacts(db).write36(facts),
        read37: facts => calendarCommandsWorkProjectionFacts(db).read37(facts),
        read38: facts => calendarCommandsWorkProjectionFacts(db).read38(facts),
        write39: facts => calendarCommandsWorkProjectionFacts(db).write39(facts),
        write40: facts => calendarCommandsWorkProjectionFacts(db).write40(facts),
        read42: facts => calendarCommandsWorkProjectionFacts(db).read42(facts),
        read44: facts => calendarCommandsWorkProjectionFacts(db).read44(facts),
        write45: facts => calendarCommandsWorkProjectionFacts(db).write45(facts),
        write48: facts => calendarCommandsWorkProjectionFacts(db).write48(facts),
        write50: facts => calendarCommandsWorkProjectionFacts(db).write50(facts),
        write52: facts => calendarCommandsWorkProjectionFacts(db).write52(facts),
        read53: facts => calendarCommandsWorkProjectionFacts(db).read53(facts),
        write54: facts => calendarCommandsWorkProjectionFacts(db).write54(facts),
        write56: facts => calendarCommandsWorkProjectionFacts(db).write56(facts),
        write57: facts => calendarCommandsWorkProjectionFacts(db).write57(facts),
        access: actor => calendarAccess(db, actor),
        needPermission: (...args) => needPermission(db, ...args),
        warnings: (...args) => appointmentWarningMessages(db, ...args),
        replay: (...args) => replayCommand<Result>(db, ...args),
        execute: (...args) => executeCommand(db, ...args),
        batch: effects => db.batch(effects),
        bindAttachments: (...args) => prepareAttachmentBinding(db, ...args),
        clearancePlan: (...args) => requireClearancePlan(db, ...args),
        clearanceCompletion: (...args) => requireClearanceCompletion(db, ...args),
        publish: input => publishInboxEvent(db, input),
        transaction: callback => (db as PostgresDatabase).transaction(tx => callback(calendarCommandsUnit(tx))) };
}
export function localInterval(db: SqlDatabase, start: string, end: string, zone: string) { return service.localInterval(calendarCommandsUnit(db), start, end, zone); }
export function calendarCommand(database: PostgresDatabase, actor: WorkContext, raw: unknown, key: string) { return service.calendarCommand(calendarCommandsUnit(database), actor, raw, key); }
export type { Result, SeriesRow } from "../../domain/calendar/commands";
