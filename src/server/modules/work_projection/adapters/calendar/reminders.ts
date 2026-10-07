import type { SqlDatabase } from "../../../runtime/public/runtime/types";
import "server-only";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { publishInboxEvent } from "@/server/modules/collaboration/public/inbox";
import type { SqlStatement } from "../../../runtime/public/runtime/types";
import { fail } from "./access";
import { calendarRemindersWorkProjectionFacts } from "../../../work_projection/public/calendar-reminders-facts";
import { createCalendarReminders, type CalendarRemindersUnit } from "../../application/calendar/reminders";
const service = createCalendarReminders<SqlStatement>({ clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, fail });
export function calendarRemindersUnit(db: SqlDatabase): CalendarRemindersUnit<SqlStatement> {
    return { read1: facts => calendarRemindersWorkProjectionFacts(db).read1(facts),
        read2: facts => calendarRemindersWorkProjectionFacts(db).read2(facts),
        read3: facts => calendarRemindersWorkProjectionFacts(db).read3(facts),
        read4: facts => calendarRemindersWorkProjectionFacts(db).read4(facts),
        write5: facts => calendarRemindersWorkProjectionFacts(db).write5(facts),
        read6: facts => calendarRemindersWorkProjectionFacts(db).read6(facts),
        publish: input => publishInboxEvent(db, input),
        transaction: callback => (db as PostgresDatabase).transaction(tx => callback(calendarRemindersUnit(tx))) };
}
export function drainCalendarReminders(db: PostgresDatabase, now = Date.now()) { return service.drainCalendarReminders(calendarRemindersUnit(db), now); }
export type { Row } from "../../domain/calendar/reminders";
