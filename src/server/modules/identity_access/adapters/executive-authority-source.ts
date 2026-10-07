import {identitySnapshotSources as organizationSnapshotSources} from '../../organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import type {SqlDatabase} from '../../runtime/public/runtime/types';import type {WorkContext} from '../public/authorization';
export async function executiveCurrentAuthoritySource(db:SqlDatabase,a:WorkContext,now:number) {
  const iam=a.iam;
  const valid=await db.prepare(`SELECT 1 FROM assignments grant_row
    ${organizationSources.executive_authority_source_organization_nodes_c_join} ON c.id=grant_row.company_id AND c.status='active'
    WHERE grant_row.id=? AND grant_row.company_id=? AND grant_row.user_id=? AND grant_row.role=?
    AND (grant_row.scope_kind='holding' OR (grant_row.scope_kind='branch' AND grant_row.scope_node_id=?) OR (grant_row.scope_kind='department' AND grant_row.scope_node_id=?))
    AND (grant_row.scope_kind='holding' OR EXISTS(SELECT 1 ${organizationSources.executive_authority_source_organization_nodes_n_from} WHERE n.id=grant_row.scope_node_id AND n.company_id=grant_row.company_id AND n.status='active' AND (n.branch_id IS NULL OR EXISTS(SELECT 1 ${organizationSources.executive_authority_source_organization_nodes_b_from} WHERE b.id=n.branch_id AND b.company_id=grant_row.company_id AND b.status='active'))))
    AND grant_row.revoked_at IS NULL AND grant_row.valid_from<=? AND grant_row.valid_until>?
    AND (?::integer IS NULL OR grant_row.version=?)
    AND iam_assignment_active(grant_row.id,?) AND iam_principal_active(grant_row.user_id,grant_row.company_id,?)
    AND EXISTS(SELECT 1 FROM iam_roles r WHERE r.id=grant_row.role AND r.company_id IN ('*',grant_row.company_id) AND r.status='active' AND (?::integer IS NULL OR r.published_revision=?))
    AND (?::integer IS NULL OR COALESCE((SELECT version FROM iam_members WHERE company_id=grant_row.company_id AND user_id=grant_row.user_id),0)=?)
    AND (?::bigint IS NULL OR ?::bigint>?)
    AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions WHERE session_id=? AND company_id=grant_row.company_id AND user_id=grant_row.user_id)
    AND (?::text IS NULL OR EXISTS(SELECT 1 FROM active_work_contexts w WHERE w.session_id=? AND w.company_id=grant_row.company_id AND w.user_id=grant_row.user_id AND w.assignment_id=grant_row.id AND w.revision=? AND w.assignment_version=grant_row.version AND w.expires_at>?))`)
    .bind(a.assignmentId,a.companyId,a.userId,a.role,a.branchId,a.departmentId,now,now,
      a.authority?.assignmentVersion??iam?.queuedAssignmentVersion??null,a.authority?.assignmentVersion??iam?.queuedAssignmentVersion??null,now,now,
      iam?.roleRevision??null,iam?.roleRevision??null,iam?.identityVersion??null,iam?.identityVersion??null,
      iam?.emergencyUntil??null,iam?.emergencyUntil??null,now,a.authority?.sessionId??null,
      a.authority?.sessionId??null,a.authority?.sessionId??null,a.authority?.revision??null,now).first();
  return !!valid;
}
