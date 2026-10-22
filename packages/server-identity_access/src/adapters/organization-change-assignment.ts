import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
const eligibleAssignment = `a.scope_kind='holding' AND a.revoked_at IS NULL
  AND iam_role_allows(a.role,a.company_id,'organization.manage')
  AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
  AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`;

export async function organizationChangeAssignmentActive(db:SqlDatabase,actor:WorkContext){
  return !!await db.prepare(`SELECT 1 FROM assignments a ${organizationSources.organization_change_assignment_organization_nodes_c_join} ON c.id=a.company_id AND c.status='active'
    WHERE a.id=? AND a.user_id=? AND a.company_id=? AND a.role=? AND ${eligibleAssignment}`)
    .bind(actor.assignmentId,actor.userId,actor.companyId,actor.role).first();
}
