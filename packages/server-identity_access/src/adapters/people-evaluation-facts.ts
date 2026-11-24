import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {samePersonColumnSql} from './iam-probe-sql';
/** Owner-local fixed projection for the People evaluation same-person probe. The
 * three facts ports previously repeated one byte-identical SQL literal; one
 * projection now serves every call site so they cannot drift apart. The native
 * identity principal reader owns the same authorization shape. */
const samePersonProbeSql = samePersonColumnSql('?','?','?');
export function peopleEvaluationIdentityAccessFacts(database: SqlDatabase) {
    return { readTransitionEvaluationAssessmentEffect(userId: string, created_by: string, companyId: string) { return database.prepare(samePersonProbeSql).bind(userId, created_by, companyId).first<boolean>("same"); },
        readTransitionEvaluationAssessmentEffect2(userId: string, reviewed_by: string, companyId: string) { return database.prepare(samePersonProbeSql).bind(userId, reviewed_by, companyId).first<boolean>("same"); },
        readApproveEvaluationCompensationEffect(userId: string, created_by: unknown, companyId: string) { return database.prepare(samePersonProbeSql).bind(userId, created_by, companyId).first<boolean>("same"); } };
}
