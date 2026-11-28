import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { ppRd19InList } from './pp-read-tail-r19-sources';
import { ppRd27CalendarAbsenceSql } from './pp-read-tail-r27-sources';
import {
  nativeCalendarPersonScope,
  type LaneHoldingScope,
} from "./native-lane-6/client";

/**
 * Owner-local calendar-index facts. `read1` (the Company person-scope presence
 * probe) now lives in the native lane-6 axum/sqlx owner; this adapter transports
 * only bounded identities and emits no SQL for it. `read3` remains on its
 * separate owner projection source.
 */
export function calendarIndexPeoplePayrollFacts(
    db: SqlDatabase,
    scope?: LaneHoldingScope,
) {
    return {
        read1: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => nativeCalendarPersonScope(scope, facts.value2, facts.value1, facts.value3)
            .then(present => present ? { present: 1 } : null),
        read3: (facts: {
            value1: string;
            value2: string;
            value3: number;
            value4: string;
            value5: number;
        }) => {
            return db.prepare(ppRd27CalendarAbsenceSql(ppRd19InList("coalesce(l.person_id,e.person_id)"))).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value4).first<Record<string, unknown>>();
        } };
}
