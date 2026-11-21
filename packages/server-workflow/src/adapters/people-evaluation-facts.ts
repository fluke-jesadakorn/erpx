import "server-only";
import {nativeWorkflowWrite} from './native-workflow-write';
import { nativeClearCommandAssertions } from '@erpx/server-collaboration/adapters/native-command-assertions';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function peopleEvaluationWorkflowFacts(database: SqlDatabase) {
    return { stageEvaluationAuthorityEffect(key: string, companyId: string) { return nativeClearCommandAssertions(database, companyId, key); },
        stageApproveEvaluationCompensationEffect6(value: string, key: string, draftId: string, op: "people.evaluation.compensation.approve", userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number) { return nativeWorkflowWrite('wf_native_people_evaluation_facts_18', [value, key, draftId, op, userId, assignmentId, expectedVersion, value2, now]); } };
}
