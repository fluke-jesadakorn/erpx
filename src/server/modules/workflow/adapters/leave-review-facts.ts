import {leaveReviewEvidenceEffects} from '../../people_payroll/public/leave-review-evidence-effects';
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import "server-only";
export function leaveReviewEvidence(database: SqlDatabase) {
    return { ...leaveReviewEvidenceEffects(database),
        stageDecideEffect3(value: string, key: string, id: string, operation: "leave.approve" | "leave.reject", userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number) {
            return database.prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(value, key, id, operation, userId, assignmentId, expectedVersion, value2, now);
        } };
}
