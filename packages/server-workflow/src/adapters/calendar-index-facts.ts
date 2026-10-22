import {workflowFinalOwnerProjections as peoplepayrollFinalOwnerProjections} from "@erpx/server-people_payroll/public/workflow-final-owner-projections";
const peoplepayrollFinalSources=peoplepayrollFinalOwnerProjections();
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
const inList = (expression: string) => `${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`;
export function calendarIndexWorkflowFacts(db: SqlDatabase) {
    return { read4: (facts: {
            value1: string;
            value2: string;
        }) => {
            return db.prepare(`SELECT record_id,action,actor,at FROM (SELECT e.record_id,e.action,coalesce(p.legal_name,'Authorized colleague') actor,e.created_at at,row_number() OVER(PARTITION BY e.record_id ORDER BY e.created_at DESC,e.id DESC) ordinal FROM audit_events e JOIN command_receipts r ON r.command_id=e.command_id LEFT ${peoplepayrollFinalSources.calendar_index_facts_hr_people_p_join} ON p.user_id=e.actor_id AND p.company_id=? AND p.merged_into_id IS NULL WHERE r.authority_json::jsonb->>'companyId'=? AND ${inList("e.record_id")}) h WHERE ordinal<=20 ORDER BY at DESC`).bind(facts.value1, facts.value1, facts.value2).all<{
                record_id: string;
                action: string;
                actor: string;
                at: number;
            }>().then(result => result.results);
        } };
}
