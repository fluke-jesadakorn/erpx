import type { SqlDatabase } from "./runtime/types";
// Database-only domain code shared by the Next.js server and jobs Worker.
import type { WorkContext } from "./authorization";
import type { RecordScope } from "./staff-policy";
import { companyAuthority } from "./staff-policy";
import { expenseAuthoritySnapshot } from "./expense-routing";
import { assertion } from "./commands";
import { ApiError } from "./http";

export const serviceMemberSql = `EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=? AND a.user_id=?
 AND a.revoked_at IS NULL AND a.valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?) OR (a.scope_kind='department' AND a.scope_node_id=?)))`;
export function serviceMemberArgs(scope: RecordScope, userId: string) {
  return [scope.company_id, userId, scope.branch_id, scope.department_id];
}
export async function serviceMember(
  db: SqlDatabase,
  scope: RecordScope,
  userId: string,
) {
  return !!(await db
    .prepare(`SELECT 1 WHERE ${serviceMemberSql}`)
    .bind(...serviceMemberArgs(scope, userId))
    .first());
}
export async function companyMember(
  db: SqlDatabase,
  companyId: string,
  userId: string,
) {
  return !!(await db
    .prepare(
      "SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND revoked_at IS NULL AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) LIMIT 1",
    )
    .bind(companyId, userId)
    .first());
}
export function inServiceWorkspace(actor: WorkContext, scope: RecordScope) {
  return (
    actor.companyId === scope.company_id &&
    (!actor.branchId || actor.branchId === scope.branch_id) &&
    (!actor.departmentId || actor.departmentId === scope.department_id)
  );
}
export async function validateServiceScope(
  db: SqlDatabase,
  actor: WorkContext,
  scope: RecordScope,
) {
  if (
    !inServiceWorkspace(actor, scope) ||
    !(await serviceMember(db, scope, actor.userId))
  )
    throw new ApiError(
      403,
      "REQUEST_SCOPE_FORBIDDEN",
      "Choose a location within your authorized workspace.",
    );
  if (
    !(await db
      .prepare(
        "SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='company' AND status='active'",
      )
      .bind(scope.company_id, scope.company_id)
      .first())
  )
    throw new ApiError(
      409,
      "COMPANY_UNAVAILABLE",
      "The company is unavailable.",
    );
  if (
    scope.branch_id &&
    !(await db
      .prepare(
        "SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'",
      )
      .bind(scope.branch_id, scope.company_id)
      .first())
  )
    throw new ApiError(422, "LOCATION_INVALID", "Select an active branch.");
  if (
    scope.department_id &&
    !(await db
      .prepare(
        "SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND branch_id IS NOT DISTINCT FROM ? AND status='active'",
      )
      .bind(scope.department_id, scope.company_id, scope.branch_id)
      .first())
  )
    throw new ApiError(
      422,
      "LOCATION_INVALID",
      "Select a department in this branch.",
    );
}
export async function serviceOwner(db: SqlDatabase, actor: WorkContext) {
  return companyAuthority(db, actor, ["holding_owner"]);
}
export async function requireServiceOwner(db: SqlDatabase, actor: WorkContext) {
  if (!(await serviceOwner(db, actor)))
    throw new ApiError(
      403,
      "OWNER_REQUIRED",
      "A company owner must configure service teams and handbook editors.",
    );
}
const setupSql = `SELECT jsonb_build_object(
 'teams',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'version',version,'active',active)) FROM (SELECT * FROM service_teams WHERE company_id=? ORDER BY id)),
 'members',(SELECT erpx_json_array_agg(jsonb_build_object('team',team_id,'user',user_id,'role',responsibility)) FROM (SELECT * FROM service_team_members WHERE company_id=? ORDER BY team_id,user_id)),
 'coverage',(SELECT erpx_json_array_agg(jsonb_build_object('team',team_id,'category',category,'kind',scope_kind,'node',scope_node_id)) FROM (SELECT * FROM service_team_coverage WHERE company_id=? ORDER BY category,scope_kind,scope_node_id)),
 'editors',(SELECT erpx_json_array_agg(user_id) FROM (SELECT user_id FROM handbook_editors WHERE company_id=? ORDER BY user_id))
) snapshot`;
export async function serviceAuthoritySnapshot(
  db: SqlDatabase,
  companyId: string,
) {
  const authority = await expenseAuthoritySnapshot(db, companyId);
  const args = [companyId, companyId, companyId, companyId];
  const row = await db
    .prepare(setupSql)
    .bind(...args)
    .first<{ snapshot: string }>();
  return (key: string) => [
    authority.guard(key, "service-member-authority"),
    assertion(db, key, "service-team-authority", `(${setupSql})=?`, [
      ...args,
      row!.snapshot,
    ]),
  ];
}
