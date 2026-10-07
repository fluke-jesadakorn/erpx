import { finalRootOwnerProjectionSources as collaborationProjectionSources } from "../../collaboration/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "../../identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/final-root-owner-projection-sources";
import { createValidateServiceScope, createRequireServiceOwner } from "../application/service-access";
export { inServiceWorkspace } from "../domain/service-access";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
// Database-only domain code shared by the Next.js server and jobs Worker.
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { RecordScope } from "@/server/modules/operations/public/staff-policy";
import { companyAuthority } from "@/server/modules/operations/public/staff-policy";
import { expenseAuthoritySnapshot } from "@/server/modules/people_payroll/public/expense-routing";
import { assertion } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
export const serviceMemberSql = `EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) a WHERE a.company_id=? AND a.user_id=?
 AND a.revoked_at IS NULL AND a.valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?) OR (a.scope_kind='department' AND a.scope_node_id=?)))`;
export function serviceMemberArgs(scope: RecordScope, userId: string) {
    return [scope.company_id, userId, scope.branch_id, scope.department_id];
}
export async function serviceMember(db: SqlDatabase, scope: RecordScope, userId: string) {
    return !!(await db
        .prepare(`SELECT 1 WHERE ${serviceMemberSql}`)
        .bind(...serviceMemberArgs(scope, userId))
        .first());
}
export async function companyMember(db: SqlDatabase, companyId: string, userId: string) {
    return !!(await db
        .prepare(`SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE company_id=? AND user_id=? AND revoked_at IS NULL AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) LIMIT 1`)
        .bind(companyId, userId)
        .first());
}
export function nativeValidateServiceScope(db: SqlDatabase) {
    return createValidateServiceScope({ error: violation => new ApiError(violation.status, violation.code, violation.message), member: (scope, userId) => serviceMember(db, scope, userId),
        async company(companyId) { return Boolean(await db.prepare(`SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND company_id=? AND kind='company' AND status='active'`).bind(companyId, companyId).first()); },
        async branch(companyId, branchId) { return Boolean(await db.prepare(`SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'`).bind(branchId, companyId).first()); },
        async department(companyId, branchId, departmentId) { return Boolean(await db.prepare(`SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND company_id=? AND kind='department' AND branch_id IS NOT DISTINCT FROM ? AND status='active'`).bind(departmentId, companyId, branchId).first()); },
    });
}
export async function validateServiceScope(db: SqlDatabase, actor: WorkContext, scope: RecordScope) { return nativeValidateServiceScope(db)(actor, scope); }
export async function serviceOwner(db: SqlDatabase, actor: WorkContext) {
    return companyAuthority(db, actor, ["holding_owner"]);
}
export async function requireServiceOwner(db: SqlDatabase, actor: WorkContext) { return createRequireServiceOwner({ owner: actor => serviceOwner(db, actor), error: violation => new ApiError(violation.status, violation.code, violation.message) })(actor); }
const setupSql = `SELECT jsonb_build_object(
 'teams',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'version',version,'active',active)) FROM (SELECT * FROM service_teams WHERE company_id=? ORDER BY id)),
 'members',(SELECT erpx_json_array_agg(jsonb_build_object('team',team_id,'user',user_id,'role',responsibility)) FROM (SELECT * FROM service_team_members WHERE company_id=? ORDER BY team_id,user_id)),
 'coverage',(SELECT erpx_json_array_agg(jsonb_build_object('team',team_id,'category',category,'kind',scope_kind,'node',scope_node_id)) FROM (SELECT * FROM service_team_coverage WHERE company_id=? ORDER BY category,scope_kind,scope_node_id)),
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
