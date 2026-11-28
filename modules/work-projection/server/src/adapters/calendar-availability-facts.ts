import "server-only";
import { nativeCalendarRead } from '@erpx/work-projection-ui/lib/native-calendar-read';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {
  lane29ContextFor,
  nativeAvailabilitySources,
  nativeMovedOccurrences,
  nativeOccurrenceException,
} from './native-lane-29/client';

/**
 * Owner-native availability reads. The axum lane 29 router executes the
 * calendar SQL, the Company scope and the projection; these adapters are a
 * strict transport only. There is no TypeScript SQL or fallback for these
 * reads: an unavailable native route fails closed with an ApiError.
 */
export function readOccurrenceException(db: SqlDatabase, facts: {
        value1: string;
        value2: string;
        value3: string;
    }) {
    return nativeOccurrenceException(lane29ContextFor(db, facts.value2), facts.value2, facts.value1, facts.value3);
}

/** Occurrence-response reads remain on the already-native calendar read port. */
export function readOccurrenceResponse(facts: {
        value1: string;
        value2: string;
        value3: string;
        value4: string;
    }) {
    return nativeCalendarRead.read61(facts);
}

export function readMovedOccurrences(db: SqlDatabase, facts: {
        value1: string;
        value2: string;
    }) {
    return nativeMovedOccurrences(lane29ContextFor(db, facts.value2), facts.value2, facts.value1)
        .then(results => ({results}));
}

export function calendarAvailabilityWorkProjectionFacts(db: SqlDatabase) {
    return {
        // The series batch read, the exception point read and the moved-occurrence
        // list are now lane 29 native reads; no SQL is rendered in TypeScript.
        read2: (facts: {
            value1: string;
            value2: number;
            value3: string[];
        }) => nativeAvailabilitySources(lane29ContextFor(db, facts.value1), facts.value1, facts.value2, facts.value3)
            .then(results => ({results: results as unknown as {
                id: string;
                starts_at: number;
                ends_at: number;
                timezone: string;
                frequency: 'daily' | 'weekly' | 'monthly' | 'yearly' | null;
                interval_count: number | null;
                weekdays: string | null;
                end_kind: 'never' | 'count' | 'until' | null;
                occurrence_count: number | null;
                until_date: string | null;
                user_id: string;
            }[]})),
        read3: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => readOccurrenceException(db, facts),
        read4: (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: string;
        }) => nativeCalendarRead.read61(facts),
        read5: (facts: {
            value1: string;
            value2: string;
        }) => readMovedOccurrences(db, facts),
        read6: (facts: {
            value1: string;
            value2: string;
            value3: string;
            value4: string;
        }) => nativeCalendarRead.read61(facts) };
}
