import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function peopleEvaluationIdentityAccessFacts(database: SqlDatabase) {
    return { readTransitionEvaluationAssessmentEffect(userId: string, created_by: string, companyId: string) { return database.prepare("SELECT iam_same_person(?,?,?) same").bind(userId, created_by, companyId).first<boolean>("same"); },
        readTransitionEvaluationAssessmentEffect2(userId: string, reviewed_by: string, companyId: string) { return database.prepare("SELECT iam_same_person(?,?,?) same").bind(userId, reviewed_by, companyId).first<boolean>("same"); },
        readApproveEvaluationCompensationEffect(userId: string, created_by: unknown, companyId: string) { return database.prepare("SELECT iam_same_person(?,?,?) same").bind(userId, created_by, companyId).first<boolean>("same"); } };
}
