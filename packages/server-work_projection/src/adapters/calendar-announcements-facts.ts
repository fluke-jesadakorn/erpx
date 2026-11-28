import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { AnnouncementRow } from "@erpx/server-work_projection/domain/calendar/announcements";
import { lane29ContextFor, nativeAnnouncementCandidates } from './native-lane-29/client';

/** The announcement candidate list is an owner-native lane 29 read. The axum
 * router owns the SQL, Company scope and projection; no TypeScript SQL remains.
 */
export function calendarAnnouncementsWorkProjectionFacts(db: SqlDatabase) {
    return { read1: (facts: {
            value1: string;
            value2: string;
            value3: boolean;
            value4: string;
            value5: string;
            value6: string;
            value7: string;
            value8: number;
        }) => nativeAnnouncementCandidates(lane29ContextFor(db, facts.value1), facts.value1, [
            facts.value1, facts.value2, facts.value3, facts.value4,
            // The Axum contract repeats the management scope and ends with a reserved value.
            facts.value4, facts.value5, facts.value6, facts.value7, facts.value8, null,
        ]).then(rows => rows as unknown as (AnnouncementRow & {owner_name: string})[]) };
}
