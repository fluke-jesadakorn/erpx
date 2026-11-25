import { finalRootOwnerProjectionSources as collaborationProjectionSources } from "@erpx/server-collaboration/public/final-root-owner-projection-sources";
import { createValidateServiceScope, createRequireServiceOwner } from "@erpx/server-support/application/service-access";
export { inServiceWorkspace } from "@erpx/server-support/domain/service-access";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
// Database-only domain code shared by the Next.js server and jobs Worker.
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { RecordScope } from "@erpx/server-operations/public/staff-policy";
import { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import { expenseAuthoritySnapshot } from "@erpx/server-people_payroll/public/expense-routing";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { ApiError } from "@erpx/server-runtime/public/http";
import { nativeSupportReadAll, SUPPORT_RD_SERVICE_SCOPE_COMPANY, SUPPORT_RD_SERVICE_SCOPE_BRANCH, SUPPORT_RD_SERVICE_SCOPE_DEPARTMENT, SUPPORT_RD_SERVICE_MEMBER, SUPPORT_RD_COMPANY_MEMBER } from "@erpx/server-support/adapters/native-support-read";
/** Scoped service-membership probe. The active-assignment read is planned by
 * the native owner as `support_rd_service_member`; only the four scope scalars
 * cross the wire and no TypeScript SQL remains. */
export async function serviceMember(db: SqlDatabase, scope: RecordScope, userId: string) {
    return Boolean((await nativeSupportReadAll(SUPPORT_RD_SERVICE_MEMBER, [scope.company_id, userId, scope.branch_id, scope.department_id], db)).results[0]);
}
/** Company-membership probe. The active-assignment read is planned by the
 * native owner as `support_rd_company_member`; only the Company and user ids
 * cross the wire and no TypeScript SQL remains. */
export async function companyMember(db: SqlDatabase, companyId: string, userId: string) {
    return Boolean((await nativeSupportReadAll(SUPPORT_RD_COMPANY_MEMBER, [companyId, userId], db)).results[0]);
}
/** Native active-organization-node scope probes. The company and branch kinds
 * carry the closed Organization node projection with the active/kind predicates
 * as fixed native SQL and two binds; the department kind adds the original
 * `branch_id IS NOT DISTINCT FROM ?` scope match as a third bind. Only scalar
 * node/company/branch ids cross the wire and the native service scope authority
 * remains. */
export function nativeValidateServiceScope(db: SqlDatabase) {
    const node = (kind: string, values: readonly unknown[]) =>
        nativeSupportReadAll<{ '1': number }>(kind, values, db).then(result => Boolean(result.results[0]));
    return createValidateServiceScope({ error: violation => new ApiError(violation.status, violation.code, violation.message), member: (scope, userId) => serviceMember(db, scope, userId),
        async company(companyId) { return node(SUPPORT_RD_SERVICE_SCOPE_COMPANY, [companyId, companyId]); },
        async branch(companyId, branchId) { return node(SUPPORT_RD_SERVICE_SCOPE_BRANCH, [branchId, companyId]); },
        async department(companyId, branchId, departmentId) { return node(SUPPORT_RD_SERVICE_SCOPE_DEPARTMENT, [departmentId, companyId, branchId]); },
    });
}
export async function validateServiceScope(db: SqlDatabase, actor: WorkContext, scope: RecordScope) { return nativeValidateServiceScope(db)(actor, scope); }
export async function serviceOwner(db: SqlDatabase, actor: WorkContext) {
    return companyAuthority(db, actor, ["holding_owner"]);
}
export async function requireServiceOwner(db: SqlDatabase, actor: WorkContext) { return createRequireServiceOwner({ owner: actor => serviceOwner(db, actor), error: violation => new ApiError(violation.status, violation.code, violation.message) })(actor); }
/** One parameterized team-set aggregate family: teams/members/coverage differ
 * only by table, projected fields and order. Rendering stays byte-identical and
 * the native service-team authority snapshot remains the authority. */
const serviceTeamSetSql = (table: string, fields: string, order: string) =>
    `(SELECT erpx_json_array_agg(jsonb_build_object(${fields})) FROM (SELECT * FROM ${table} WHERE company_id=? ORDER BY ${order}))`;
const setupSql = `SELECT jsonb_build_object(
 'teams',${serviceTeamSetSql('service_teams', "'id',id,'version',version,'active',active", 'id')},
 'members',${serviceTeamSetSql('service_team_members', "'team',team_id,'user',user_id,'role',responsibility", 'team_id,user_id')},
 'coverage',${serviceTeamSetSql('service_team_coverage', "'team',team_id,'category',category,'kind',scope_kind,'node',scope_node_id", 'category,scope_kind,scope_node_id')},
 'editors',(SELECT erpx_json_array_agg(user_id) FROM (SELECT user_id FROM (${collaborationProjectionSources.handbook_editors}) handbook_editors WHERE company_id=? ORDER BY user_id))
) snapshot`;
export async function serviceAuthoritySnapshot(db: SqlDatabase, companyId: string) {
    const authority = await expenseAuthoritySnapshot(db, companyId);
    const args = [companyId, companyId, companyId, companyId];
    const row = await db
        .prepare(setupSql)
        .bind(...args)
        .first<{
        snapshot: string;
    }>();
    return (key: string) => [
        authority.guard(key, "service-member-authority"),
        assertion(db, key, "service-team-authority", `(${setupSql})=?`, [
            ...args,
            row!.snapshot,
        ]),
    ];
}
