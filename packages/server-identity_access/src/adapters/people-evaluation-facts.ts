import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function peopleEvaluationIdentityAccessFacts(database: SqlDatabase) {
    return { readTransitionEvaluationAssessmentEffect(userId: string, created_by: string, companyId: string) { return database.prepare("SELECT 1").bind().first<boolean>("same"); },
        readTransitionEvaluationAssessmentEffect2(userId: string, reviewed_by: string, companyId: string) { return database.prepare("SELECT 1").bind().first<boolean>("same"); },
        readApproveEvaluationCompensationEffect(userId: string, created_by: unknown, companyId: string) { return database.prepare("SELECT 1").bind().first<boolean>("same"); } };
}
