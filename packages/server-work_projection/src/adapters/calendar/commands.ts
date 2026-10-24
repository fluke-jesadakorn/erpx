import {withPeoplePreparationTransaction} from "@erpx/server-people_payroll/public/people-purchases";
import {requireClearanceCompletion,requireClearancePlan} from '@erpx/server-people_payroll/public/retirement-clearance';
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { executeCommand, replayCommand } from "@erpx/server-workflow/public/commands";
import { rescheduleRosterShift } from "@erpx/server-people_payroll/public/people-roster";
import { publishInboxEvent } from "@erpx/server-collaboration/public/inbox";
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import type { SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import type { Result } from "@erpx/server-work_projection/domain/calendar/commands";
import { calendarAccess, needPermission, requirePlacement, fail } from "@erpx/server-work_projection/adapters/calendar/access";
import { appointmentWarningMessages } from "@erpx/server-work_projection/adapters/calendar/appointments";
import { calendarCommandsWorkProjectionFacts } from "@erpx/server-work_projection/public/calendar-commands-facts";
import { calendarCommandsOperationsFacts } from "@erpx/server-operations/public/calendar-commands-facts";
import { calendarCommandsPeoplePayrollFacts } from "@erpx/server-people_payroll/public/calendar-commands-facts";
import { calendarCommandsIdentityAccessFacts } from "@erpx/server-identity_access/public/calendar-commands-facts";
import { createCalendarCommands, type CalendarCommandsUnit } from "@erpx/server-work_projection/application/calendar/commands";
import { withRosterPurchasedTransaction } from "@erpx/server-people_payroll/public/roster-purchases";
import {isManualCalendarCommand,withCalendarPurchasedTransaction} from '@erpx/server-work_projection/adapters/calendar/purchases';
import {calendarCommandSchema,type CalendarCommand} from '@erpx/server-work_projection/public/rules/calendar-domain';
const service = createCalendarCommands<SqlStatement>({ clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, fail, requirePlacement });
export function calendarCommandsUnit(db: SqlDatabase, roster?: {actor: WorkContext; key: string},people?:{actor:WorkContext;key:string;input:{id:string;action:"task.plan"|"task.complete"}},manual?:{actor:WorkContext;key:string;input:CalendarCommand}): CalendarCommandsUnit<SqlStatement> {
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
        transaction: callback => roster
            ? withRosterPurchasedTransaction(db, roster.actor, {key: roster.key, operation: "calendar.shift.reschedule", purpose: "settle-existing"}, tx => callback(calendarCommandsUnit(tx)))
            : people?withPeoplePreparationTransaction(db,people.actor,people.input,people.key,tx=>callback(calendarCommandsUnit(tx))):manual?withCalendarPurchasedTransaction(db,manual.actor,manual.input,manual.key,tx=>callback(calendarCommandsUnit(tx))):(db as PostgresDatabase).transaction(tx => callback(calendarCommandsUnit(tx))) };
}
export function localInterval(db: SqlDatabase, start: string, end: string, zone: string) { return service.localInterval(calendarCommandsUnit(db), start, end, zone); }
export function calendarCommand(database: PostgresDatabase, actor: WorkContext, raw: unknown, key: string) {
    const roster = raw && typeof raw === "object" && "action" in raw && raw.action === "shift.reschedule" ? {actor, key} : undefined;
    const people=raw&&typeof raw==="object"&&"action"in raw&&["task.plan","task.complete"].includes(String(raw.action))&&"id"in raw?{actor,key,input:{id:String(raw.id),action:raw.action as "task.plan"|"task.complete"}}:undefined;
    const input=calendarCommandSchema.parse(raw),manual=isManualCalendarCommand(input)?{actor,key,input}:undefined;
    return service.calendarCommand(calendarCommandsUnit(database, roster,people,manual), actor, input, key);
}
export type { Result, SeriesRow } from "@erpx/server-work_projection/domain/calendar/commands";
