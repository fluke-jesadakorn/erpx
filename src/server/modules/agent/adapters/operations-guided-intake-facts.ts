import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { assertion } from "@/server/modules/workflow/public/commands";
export function GuidedIntakeAgentFacts(database: SqlDatabase) {
    return { readPrepareGuidedIntakePrevious(id: string) {
            return database.prepare("SELECT t.user_id,t.company_id,t.assignment_id,t.scope_key,t.question,t.paths_json,t.state,t.sources_json,t.answer_json,t.private_session_id,e.governance_snapshot_json FROM ai_workspace_turns t LEFT JOIN ai_invocation_evidence e ON e.turn_id=t.id WHERE t.id=?")
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
        guard3(id: string, value: "assistant-rate", companyId: string, value2: number) { return assertion(database, id, value, "(SELECT count(*) FROM ai_workspace_turns WHERE company_id=? AND created_at>?)<20", [companyId, value2]); },
        stagePrepareGuidedIntakeEffect(id: string, companyId: string, userId: string, assignmentId: string, value: string, value2: string, message: string, value3: string, now: number, now2: number, sessionId: string) {
            return database.prepare("INSERT INTO ai_workspace_turns(id,company_id,user_id,assignment_id,scope_key,conversation_key,question,paths_json,state,created_at,updated_at,private_session_id) VALUES(?,?,?,?,?,?,?,?,'running',?,?,?)")
                .bind(id, companyId, userId, assignmentId, value, value2, message, value3, now, now2, sessionId);
        },
        stagePrepareGuidedIntakeEffect3(value: string, sourcesJson: string, now: number, id: string) {
            return database.prepare("UPDATE ai_workspace_turns SET state='ready',answer_json=?,sources_json=?,updated_at=? WHERE id=? AND state='running'")
                .bind(value, sourcesJson, now, id);
        },
        stagePrepareGuidedIntakeEffect4(value: string, now: number, id: string) {
            return database.prepare("UPDATE ai_workspace_turns SET state='failed',error=?,updated_at=? WHERE id=? AND state='running'")
                .bind(value, now, id);
        } };
}
