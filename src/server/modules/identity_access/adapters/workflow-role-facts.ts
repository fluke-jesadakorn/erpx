import {identitySnapshotSources as organizationSnapshotSources} from '../../organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { activeStaffSelect, type RecordScope } from "@/server/modules/operations/public/staff-policy";
import { policyRead } from "../public/iam-read-snapshot";

/** Scoped workflow-role account facts retain the operation snapshot cache. */
export async function workflowStaffRoleUsers(db: SqlDatabase, scope: RecordScope, role: string | null) {
  return (await policyRead(db, JSON.stringify(["role-users", scope.company_id, scope.branch_id, scope.department_id, role]), () => readWorkflowStaffRoleUsers(db, scope, role))).slice();
}
async function readWorkflowStaffRoleUsers(
  db: SqlDatabase,
  scope: RecordScope,
  role: string | null,
) {
  const now = Date.now();
  const rows = await db
    .prepare(
      `SELECT staff.user_id FROM (${activeStaffSelect}) staff WHERE EXISTS (
    SELECT 1 FROM assignments a WHERE a.user_id=staff.user_id AND a.company_id=? AND (?::text IS NULL OR iam_role_allows(a.role,a.company_id,'workflow.'||?)) AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=? AND a.valid_until>?
    AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?) OR (a.scope_kind='department' AND a.scope_node_id=?)))
    AND (?::text IS NULL OR EXISTS(SELECT 1 ${organizationSources.workflow_role_facts_organization_nodes_from} WHERE id=? AND company_id=staff.company_id AND kind='branch' AND status='active'))
    AND (?::text IS NULL OR EXISTS(SELECT 1 ${organizationSources.workflow_role_facts_organization_nodes_from} WHERE id=? AND company_id=staff.company_id AND kind='department' AND status='active'))
    GROUP BY staff.user_id HAVING COUNT(*)=1 ORDER BY staff.user_id`,
    )
    .bind(
      scope.company_id,
      now,
      now,
      scope.company_id,
      role,
      role,
      now,
      now,
      scope.branch_id,
      scope.department_id,
      scope.branch_id,
      scope.branch_id,
      scope.department_id,
      scope.department_id,
    )
    .all<{ user_id: string }>();
  return rows.results.map((row) => row.user_id);
}

