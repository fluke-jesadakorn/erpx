type CalendarReadResult<Row> = {
    results: Row[];
};
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { expandCalendarSeries, type RecurrenceRule } from "@/lib/work_projection/calendar-recurrence";
import { localMinuteAt } from "@/lib/work_projection/calendar-timeline";
import { localDateTimeToInstant } from "@/lib/work_projection/calendar-timeline";
import { addDays } from "@/server/modules/people_payroll/public/rules/people-roster-domain";
import type { Clock } from "../../../shared/application/ports";
export type CalendarAvailabilityUnit = {
    read1(facts: {
        value1: string;
        value2: string[];
        value3: number;
    }): Promise<CalendarReadResult<{
        user_id: string;
    }>>;
    read2(facts: {
        value1: string;
        value2: number;
        value3: string[];
    }): Promise<CalendarReadResult<{
        id: string;
        starts_at: number;
        ends_at: number;
        timezone: string;
        frequency: RecurrenceRule["frequency"] | null;
        interval_count: number | null;
        weekdays: string | null;
        end_kind: RecurrenceRule["endKind"] | null;
        occurrence_count: number | null;
        until_date: string | null;
        user_id: string;
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
    read5(facts: {
        value1: string;
        value2: string;
    }): Promise<CalendarReadResult<{
        occurrence_key: string;
        start_local: string;
        end_local: string;
    }>>;
    read6(facts: {
        value1: string;
        value2: string;
        value3: string;
        value4: string;
    }): Promise<{
        response: string;
    } | null>;
    read7(facts: {
        value1: string;
        value2: string[];
        value3: string;
        value4: string;
    }): Promise<CalendarReadResult<{
        user_id: string;
        leave_date: string;
    }>>;
};
export function createCalendarAvailability(ports: {
    clock: Clock;
    error(status: number, code: string, message: string): Error;
}) {
    /** The response contains busy intervals only. Titles, guests and reasons never leave this service. */
    async function calendarAvailability(db: CalendarAvailabilityUnit, actor: WorkContext, ids: string[], from: number, to: number) {
        const userIds = [...new Set(ids)].slice(0, 100);
        if (!userIds.length || !Number.isSafeInteger(from) || !Number.isSafeInteger(to) || to <= from || to - from > 31 * 86400000)
            throw ports.error(422, "AVAILABILITY_RANGE", "Choose people and a range of up to 31 days.");
        const now = ports.clock.now();
        const active = (await db.read1({ value1: actor.companyId, value2: userIds, value3: now })).results;
        if (active.length !== userIds.length)
            throw ports.error(404, "INVITEE_UNAVAILABLE", "Choose active Company accounts.");
        const appointments = (await db.read2({ value1: actor.companyId, value2: to, value3: userIds })).results;
        const intervals: {
            userId: string;
            startsAt: number;
            endsAt: number;
        }[] = [];
        for (const row of appointments) {
            if (!row.frequency) {
                if (row.ends_at > from)
                    intervals.push({ userId: row.user_id, startsAt: Math.max(from, row.starts_at), endsAt: Math.min(to, row.ends_at) });
                continue;
            }
            const start = localMinuteAt(row.starts_at, row.timezone), end = localMinuteAt(row.ends_at, row.timezone);
            const rule: RecurrenceRule = { frequency: row.frequency, intervalCount: row.interval_count!, weekdays: row.weekdays ? row.weekdays.split(",").map(Number) : [],
                endKind: row.end_kind!, occurrenceCount: row.occurrence_count, untilDate: row.until_date };
            const considered = new Set<string>();
            for (const occurrence of expandCalendarSeries(`${start.date}T${start.time}`, `${end.date}T${end.time}`, row.timezone, rule, from, to, 500)) {
                considered.add(occurrence.key);
                const exception = await db.read3({ value1: row.id, value2: actor.companyId, value3: occurrence.key });
                if (exception?.cancelled)
                    continue;
                const response = await db.read4({ value1: row.id, value2: actor.companyId, value3: occurrence.key, value4: row.user_id });
                if (response?.response === "declined")
                    continue;
                let startsAt = occurrence.startsAt, endsAt = occurrence.endsAt;
                if (exception?.start_local && exception.end_local) {
                    try {
                        startsAt = localDateTimeToInstant(exception.start_local, row.timezone);
                        endsAt = localDateTimeToInstant(exception.end_local, row.timezone);
                    }
                    catch {
                        continue;
                    }
                }
                if (startsAt < to && endsAt > from)
                    intervals.push({ userId: row.user_id, startsAt: Math.max(from, startsAt), endsAt: Math.min(to, endsAt) });
            }
            const moved = (await db.read5({ value1: row.id, value2: actor.companyId })).results;
            for (const change of moved) {
                if (considered.has(change.occurrence_key))
                    continue;
                const response = await db.read6({ value1: row.id, value2: actor.companyId, value3: change.occurrence_key, value4: row.user_id });
                if (response?.response === "declined")
                    continue;
                try {
                    const startsAt = localDateTimeToInstant(change.start_local, row.timezone), endsAt = localDateTimeToInstant(change.end_local, row.timezone);
                    if (startsAt < to && endsAt > from)
                        intervals.push({ userId: row.user_id, startsAt: Math.max(from, startsAt), endsAt: Math.min(to, endsAt) });
                }
                catch { /* Invalid wall-clock exception is ignored. */ }
            }
        }
        const leave = (await db.read7({ value1: actor.companyId, value2: userIds, value3: new Date(from - 86400000).toISOString().slice(0, 10), value4: new Date(to + 86400000).toISOString().slice(0, 10) })).results;
        for (const row of leave) {
            try {
                const start = localDateTimeToInstant(`${row.leave_date}T00:00`, "Asia/Bangkok"), end = localDateTimeToInstant(`${addDays(row.leave_date, 1)}T00:00`, "Asia/Bangkok");
                if (start < to && end > from)
                    intervals.push({ userId: row.user_id, startsAt: Math.max(from, start), endsAt: Math.min(to, end) });
            }
            catch { /* Invalid local midnight is omitted. */ }
        }
        return { intervals: intervals.sort((a, b) => a.startsAt - b.startsAt), from, to };
    }
    return { calendarAvailability };
}
