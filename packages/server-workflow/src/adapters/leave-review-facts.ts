import {leaveReviewEvidenceEffects} from '@erpx/server-people_payroll/public/leave-review-evidence-effects';
import {nativeWorkflowWrite} from './native-workflow-write';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
export function leaveReviewEvidence(database: SqlDatabase) {
    return { ...leaveReviewEvidenceEffects(database),
        stageDecideEffect3(value: string, key: string, id: string, operation: "leave.approve" | "leave.reject", userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number) {
            return nativeWorkflowWrite('wf_native_leave_review_facts_10', [value, key, id, operation, userId, assignmentId, expectedVersion, value2, now]);
        } };
}
