import "server-only";
import { nativeCalendarRead } from '@erpx/work-projection-ui/lib/native-calendar-read';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { type RecurrenceRule } from "@erpx/work-projection-ui/lib/calendar-recurrence";
import type { AppointmentRow } from "@erpx/work-projection-server/domain/calendar/appointments";
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import {
  lane29ContextBound,
  lane29ContextFor,
  nativeAppointmentCandidates,
  nativeAttendeeRows,
  nativeConflictProbe,
  nativeSeriesBatch,
  type NativeConflictScope,
} from './native-lane-29/client';

/**
 * Owner-native calendar fact reads. The axum lane 29 router owns every SQL
 * statement, Company scope and response shape; this adapter is strict transport
 * only. No TypeScript SQL or fallback remains for these reads.
 */
function appointmentIds(value: string): string[] {
    try {
        const parsed: unknown = JSON.parse(value);
        if (Array.isArray(parsed) && parsed.length > 0 && parsed.length <= 64 &&
            parsed.every(id => typeof id === 'string' && id.length > 0 && id.length <= 200))
            return parsed as string[];
    } catch { /* fall through to the named failure */ }
    throw new ApiError(422, 'LANE29_SERIES_REQUEST_INVALID', 'Use between one and 64 bounded appointments.');
}

/** Conflict-probe recipient list may be empty; an empty list has no conflicts. */
function probeIds(value: string): string[] {
    try {
        const parsed: unknown = JSON.parse(value);
        if (Array.isArray(parsed) && parsed.length <= 64 &&
            parsed.every(id => typeof id === 'string' && id.length > 0 && id.length <= 200))
            return parsed as string[];
    } catch { /* fall through to the named failure */ }
    throw new ApiError(422, 'LANE29_CONFLICT_REQUEST_INVALID', 'Use at most 64 bounded recipients.');
}

/** The conflict probes are native lane 29 reads; no TypeScript SQL remains. */
function conflictProbe(db: SqlDatabase, scope: NativeConflictScope, facts: {
    value1: string; value2: string; value3: number; value4: number; value5: string;
}) {
    return nativeConflictProbe(
        lane29ContextFor(db, facts.value1), facts.value1, scope, facts.value2,
        facts.value3, facts.value4, probeIds(facts.value5),
    ).then(probe => probe.conflict ? {conflict: true} : null);
}

export function calendarAppointmentsWorkProjectionFacts(db: SqlDatabase) {
    return {
        read1: (facts: {
            value1: boolean;
            value2: string;
            value3: string | null;
            value4: string;
            value5: string;
            value6: number;
            value7: number;
            value8: boolean;
            value9: string | null;
            value10: "mine" | "team";
            value11: string | null;
            value12: string;
            value13: number;
        }) => nativeAppointmentCandidates(lane29ContextFor(db, facts.value5), facts.value5, [
            facts.value1, facts.value2, facts.value3, facts.value4, facts.value5,
            facts.value6, facts.value7, facts.value2, facts.value4, facts.value2,
            facts.value8, facts.value9, facts.value9, facts.value9, facts.value10,
            facts.value4, facts.value11, facts.value12, facts.value12, facts.value13,
        ]).then(rows => rows as unknown as (AppointmentRow & {full_detail: boolean; organizer_name: string | null})[]),
        read2: (facts: {value1: string; value2: string;}) =>
            nativeAttendeeRows(lane29ContextBound(db), facts.value1, [facts.value1, facts.value2])
                .then(rows => rows as unknown as {
                    appointment_id: string; user_id: string; person_id: string | null;
                    name: string | null; in_scope: boolean;
                    attendance: "organizer" | "required" | "optional";
                    response: "pending" | "accepted" | "tentative" | "declined";
                }[]),
        read3: (facts: {value1: string; value2: string;}) =>
            nativeSeriesBatch(lane29ContextFor(db, facts.value1), facts.value1, appointmentIds(facts.value2))
                .then(results => ({results: results as unknown as {
                    appointment_id: string;
                    frequency: RecurrenceRule["frequency"];
                    interval_count: number;
                    weekdays: string;
                    end_kind: RecurrenceRule["endKind"];
                    occurrence_count: number | null;
                    until_date: string | null;
                }[]})),
        read4: async (facts: {value1: string; value2: string;}) => ({ results: await nativeCalendarRead.read63(facts) }),
        read5: async (facts: {value1: string; value2: string;}) => ({ results: await nativeCalendarRead.read64(facts) }),
        read6: (facts: {value1: string; value2: string; value3: number; value4: number; value5: string;}) =>
            conflictProbe(db, 'participant', facts),
        read8: (facts: {value1: string; value2: string; value3: number; value4: number; value5: string;}) =>
            conflictProbe(db, 'attendee', facts) };
}
