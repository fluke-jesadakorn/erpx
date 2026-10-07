import {calendarPeopleProjectionSources} from "../../people_payroll/public/calendar-people-projection-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarIndexWorkProjectionFacts(db: SqlDatabase) {
    return { read2: (facts: {
            value1: string;
            value2: string;
            value3: string;
        }) => { return db.prepare("SELECT (extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint from_ms,(extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint to_ms").bind(facts.value1, facts.value2, facts.value3, facts.value2).first<{
            from_ms: number;
            to_ms: number;
        }>(); },
        read5: (facts: {
            value1: string;
            value2: string;
        }) => {
            return db.prepare(`SELECT h.record_id,h.action,coalesce(p.legal_name,'Authorized colleague') actor,h.created_at at FROM (SELECT *,row_number() OVER(PARTITION BY record_id ORDER BY created_at DESC,id DESC) ordinal FROM calendar_history WHERE company_id=? AND ${inList("record_id")}) h LEFT JOIN (${calendarPeopleProjectionSources.people}) p ON p.user_id=h.actor_id AND p.company_id=h.company_id AND p.merged_into_id IS NULL WHERE ordinal<=20 ORDER BY h.created_at DESC`).bind(facts.value1, facts.value2).all<{
                record_id: string;
                action: string;
                actor: string;
                at: number;
            }>().then(result => result.results);
        } };
}
