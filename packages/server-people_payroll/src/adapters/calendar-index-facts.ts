import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ppRd19InList as inList } from './pp-read-tail-r19-sources';
import { ppRd27CalendarAbsenceSql } from './pp-read-tail-r27-sources';
export function calendarIndexPeoplePayrollFacts(db: SqlDatabase) {
    return { read1: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => { return db.prepare(`SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.id=? AND p.company_id=? AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")} LIMIT 1`).bind(facts.value1, facts.value2, facts.value3).first<Record<string, unknown>>(); },
        read3: (facts: {
            value1: string;
            value2: string;
            value3: number;
            value4: string;
            value5: number;
        }) => {
            return db.prepare(ppRd27CalendarAbsenceSql(inList("coalesce(l.person_id,e.person_id)"))).bind(facts.value1, facts.value2, facts.value3, facts.value4, facts.value5, facts.value4).first<Record<string, unknown>>();
        } };
}
