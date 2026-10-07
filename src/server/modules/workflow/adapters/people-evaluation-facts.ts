import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function peopleEvaluationWorkflowFacts(database: SqlDatabase) {
    return { stageEvaluationAuthorityEffect(key: string) { return database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key); },
        stageApproveEvaluationCompensationEffect6(value: string, key: string, draftId: string, op: "people.evaluation.compensation.approve", userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number) { return database.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(value, key, draftId, op, userId, assignmentId, expectedVersion, value2, now); } };
}
