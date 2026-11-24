import { nativeOwnerWrite, nativeOwnerWriteNoopSql } from './native-owner-write';
import { agentTurnEvidenceSql } from './turn-evidence-projection-source';
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
/** The one owner-authored fixed head for Agent workspace-turn count guards.
 * The comparator is the only textual difference between the rate guard and the
 * executive quota guard; each call site keeps its own two binds. */
export const agentTurnCountGuardSql = (op: '>=' | '>'): string =>
  `(SELECT count(*) FROM ai_workspace_turns WHERE company_id=? AND created_at${op}?)`;
/** The one owner-authored fixed SQL for the Agent assistant-rate guard.
 * Every rate guard reuses this text; no adapter authors its own literal. */
export const agentAssistantRateGuardSql = `${agentTurnCountGuardSql('>')}<20`;
export function GuidedIntakeAgentFacts(database: SqlDatabase) {
    return { readPrepareGuidedIntakePrevious(id: string) {
            return database.prepare(`${agentTurnEvidenceSql("t.user_id,t.company_id,t.assignment_id,t.scope_key,t.question,t.paths_json,t.state,t.sources_json,t.answer_json,t.private_session_id,e.governance_snapshot_json")} WHERE t.id=?`)
                .bind(id).first<{
                user_id: string;
                company_id: string;
                assignment_id: string;
                scope_key: string;
                question: string;
                paths_json: string;
                state: string;
                sources_json: string | null;
                answer_json: string | null;
                private_session_id: string | null;
                governance_snapshot_json: string | null;
            }>();
        },
        guard3(id: string, value: "assistant-rate", companyId: string, value2: number) { return assertion(database, id, value, agentAssistantRateGuardSql, [companyId, value2]); },
        async stagePrepareGuidedIntakeEffect(id: string, companyId: string, userId: string, assignmentId: string, value: string, value2: string, message: string, value3: string, now: number, now2: number, sessionId: string) {
            return nativeOwnerWrite('ag_workspace_turn_insert',[id,companyId,userId,assignmentId,value,value2,message,value3,now,now2,sessionId],`ag-guided-turn:${id}`,{companyId,now},()=>database.prepare(nativeOwnerWriteNoopSql));
        },
        async stagePrepareGuidedIntakeEffect3(value: string, sourcesJson: string, now: number, id: string) {
            return nativeOwnerWrite('ag_workspace_turn_ready',[value,sourcesJson,now,id],`ag-guided-ready:${id}`,{now},()=>database.prepare(nativeOwnerWriteNoopSql));
        },
        async stagePrepareGuidedIntakeEffect4(value: string, now: number, id: string) {
            return nativeOwnerWrite('ag_workspace_turn_failed',[value,now,id],`ag-guided-failed:${id}`,{now},()=>database.prepare(nativeOwnerWriteNoopSql));
        } };
}
