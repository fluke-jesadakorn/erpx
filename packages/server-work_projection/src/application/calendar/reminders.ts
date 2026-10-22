type CalendarReadResult<Row> = {
    results: Row[];
};
import { expandCalendarSeries } from "@erpx/feature-work_projection/lib/calendar-recurrence";
import { localDateTimeToInstant, localMinuteAt } from "@erpx/feature-work_projection/lib/calendar-timeline";
import type { Row } from "@erpx/server-work_projection/domain/calendar/reminders";
import type { Clock, IdentifierSource } from "@erpx/server-shared/application/ports";
export type CalendarRemindersUnit<Effect> = {
    read1(facts: {
        value1: number;
        value2: number;
        value3: string;
    }): Promise<CalendarReadResult<Row>>;
    read2(facts: {
        value1: string;
        value2: string;
    }): Promise<CalendarReadResult<{
        occurrence_key: string;
        start_local: string;
        end_local: string;
    }>>;
    read3(facts: {
        value1: string;
        value2: string;
        value3: string;
    }): Promise<{
        start_local: string | null;
        end_local: string | null;
        cancelled: boolean;
    } | null>;
    read4(facts: {
        value1: string;
        value2: string;
        value3: string;
        value4: string;
    }): Promise<{
        response: string;
    } | null>;
    write5(facts: {
        value1: string;
        value2: string;
        value3: string;
        value4: string;
        value5: number;
    }): Promise<unknown>;
    read6(facts: {
        value1: number;
        value2: string;
        value3: string;
        value4: string;
        value5: number;
    }): Promise<Record<string, unknown> | null>;
    publish(input: import("@erpx/server-collaboration/public/inbox").InboxEventInput): Promise<string>;
    transaction<T>(callback: (tx: CalendarRemindersUnit<Effect>) => Promise<T>): Promise<T>;
    deliver?(row:Row,occurrence:{key:string;startsAt:number;due:number;now:number},callback:(tx:CalendarRemindersUnit<Effect>)=>Promise<boolean>):Promise<boolean>;
};
export function createCalendarReminders<Effect>(ports: {
    clock: Clock;
    identifiers: IdentifierSource;
    fail(message: string, code?: string, status?: number): never;
}) {
    /** Scheduled worker; the reminder ledger and event key make repeated runs safe. */
    async function drainCalendarReminders(db: CalendarRemindersUnit<Effect>, now = ports.clock.now()) {
        let after = "", sent = 0;
        for (;;) {
            const rows = (await db.read1({ value1: now + 8 * 86400000, value2: now - 3600000, value3: after })).results;
            if (!rows.length)
                break;
            for (const row of rows) {
                after = `${row.id}:${row.user_id}`;
                const start = localMinuteAt(row.starts_at, row.timezone), end = localMinuteAt(row.ends_at, row.timezone);
                const occurrences = row.frequency ? expandCalendarSeries(`${start.date}T${start.time}`, `${end.date}T${end.time}`, row.timezone, { frequency: row.frequency, intervalCount: row.interval_count!, weekdays: row.weekdays ? row.weekdays.split(",").map(Number) : [],
                    endKind: row.end_kind!, occurrenceCount: row.occurrence_count, untilDate: row.until_date }, now - 3600000, now + 8 * 86400000, 1000) :
                    [{ key: `${start.date}T${start.time}`, startsAt: row.starts_at, endsAt: row.ends_at }];
                if (row.frequency) {
                    const known = new Set(occurrences.map(value => value.key));
                    const moved = (await db.read2({ value1: row.id, value2: row.company_id })).results;
                    for (const change of moved) {
                        if (known.has(change.occurrence_key))
                            continue;
                        try {
                            const startsAt = localDateTimeToInstant(change.start_local, row.timezone);
                            if (startsAt < now - 3600000 || startsAt >= now + 8 * 86400000)
                                continue;
                            occurrences.push({ key: change.occurrence_key, startLocal: change.start_local, endLocal: change.end_local,
                                startsAt, endsAt: localDateTimeToInstant(change.end_local, row.timezone) });
                        }
                        catch { /* Ignore a nonexistent local time. */ }
                    }
                }
                for (const occurrence of occurrences) {
                    const exception = await db.read3({ value1: row.id, value2: row.company_id, value3: occurrence.key });
                    if (exception?.cancelled)
                        continue;
                    let startsAt = occurrence.startsAt;
                    if (exception?.start_local) {
                        try {
                            startsAt = localDateTimeToInstant(exception.start_local, row.timezone);
                        }
                        catch {
                            continue;
                        }
                    }
                    const response = await db.read4({ value1: row.id, value2: row.company_id, value3: occurrence.key, value4: row.user_id });
                    if (response?.response === "declined")
                        continue;
                    const due = startsAt - row.reminder_minutes * 60000;
                    if (due > now || startsAt < now - 3600000)
                        continue;
                    const deliver=async (tx:CalendarRemindersUnit<Effect>) => {
                        await tx.write5({ value1: row.company_id, value2: row.id, value3: occurrence.key, value4: row.user_id, value5: due });
                        const claimed = await tx.read6({ value1: now, value2: row.id, value3: occurrence.key, value4: row.user_id, value5: due });
                        if (!claimed)
                            return false;
                        await tx.publish({ key: `reminder:${row.id}:${occurrence.key}:${row.user_id}:${due}`, companyId: row.company_id,
                            sourceType: "calendar_appointment", sourceId: row.id, sourceVersion: row.version, category: "calendar", preview: `Meeting soon: ${row.title}`,
                            recipientIds: [row.user_id], createdAt: now });
                        return true;
                    };
                    if(await (db.deliver?db.deliver(row,{key:occurrence.key,startsAt,due,now},deliver):db.transaction(deliver)))sent++;
                }
            }
            if (rows.length < 200)
                break;
        }
        return sent;
    }
    return { drainCalendarReminders };
}
