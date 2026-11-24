import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ppRd19InList as inList } from './pp-read-tail-r19-sources';
import { ppRd27CalendarAbsenceSql } from './pp-read-tail-r27-sources';
export function calendarAppointmentsPeoplePayrollFacts(db: SqlDatabase) {
    return { read7: (facts: {
            value1: string;
            value2: string;
            value3: number;
            value4: string;
            value5: number;
        }) => {
            return db.prepare(ppRd27CalendarAbsenceSql(inList("coalesce(l.person_id,e.person_id)"))).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value4).first<Record<string, unknown>>();
        } };
}
