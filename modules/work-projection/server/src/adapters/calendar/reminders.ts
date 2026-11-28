import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import "server-only";
import type { PostgresDatabase } from "@erpx/platform-runtime-server/public/runtime/postgres";
import { publishInboxEvent } from "@erpx/collaboration-server/public/inbox";
import type { SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import { fail } from "@erpx/work-projection-server/adapters/calendar/access";
import { calendarRemindersWorkProjectionFacts } from "@erpx/work-projection-server/public/calendar-reminders-facts";
import { createCalendarReminders, type CalendarRemindersUnit } from "@erpx/work-projection-server/application/calendar/reminders";
import {withCalendarReminderDelivery} from '@erpx/work-projection-server/adapters/calendar/reminder-purchases';
import {isTransactionalSqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
const service = createCalendarReminders<SqlStatement>({ clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, fail });
export function calendarRemindersUnit(db: SqlDatabase): CalendarRemindersUnit<SqlStatement> {
    return { read1: facts => calendarRemindersWorkProjectionFacts(db).read1(facts),
        read2: facts => calendarRemindersWorkProjectionFacts(db).read2(facts),
        read3: facts => calendarRemindersWorkProjectionFacts(db).read3(facts),
        read4: facts => calendarRemindersWorkProjectionFacts(db).read4(facts),
        write5: facts => calendarRemindersWorkProjectionFacts(db).write5(facts),
        read6: facts => calendarRemindersWorkProjectionFacts(db).read6(facts),
        publish: input => publishInboxEvent(db, input),
        transaction: callback => (db as PostgresDatabase).transaction(tx => callback(calendarRemindersUnit(tx))),
        deliver:(row,occurrence,callback)=>withCalendarReminderDelivery(db,row,occurrence,tx=>tx===db?(db as PostgresDatabase).transaction(owned=>callback(calendarRemindersUnit(owned))):callback(calendarRemindersUnit(tx))) };
}
export async function drainCalendarReminders(db: SqlDatabase, now = Date.now()) { if(!isTransactionalSqlDatabase(db))throw new ApiError(503,'CALENDAR_REMINDER_UNIT_REQUIRED','Calendar reminders require their complete owned Holding transaction.');return service.drainCalendarReminders(calendarRemindersUnit(db), now); }
export type { Row } from "@erpx/work-projection-server/domain/calendar/reminders";
