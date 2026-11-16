import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function peopleEvaluationWorkflowFacts(database: SqlDatabase) {
    return { stageEvaluationAuthorityEffect(key: string) { return database.prepare("SELECT 1").bind(); },
        stageApproveEvaluationCompensationEffect6(value: string, key: string, draftId: string, op: "people.evaluation.compensation.approve", userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number) { return database.prepare("SELECT 1").bind(); } };
}
