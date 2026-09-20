import "server-only";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { ApiError } from "../http";

export const executiveScope = (a:WorkContext) => JSON.stringify([a.companyId,a.assignmentId,a.branchId,a.departmentId,a.iam?.roleRevision,a.iam?.identityVersion]);
/** Read-only authority check: a stream must not take mutation locks on every heartbeat. */
export async function currentAuthority(db:SqlDatabase,a:WorkContext) {
  const now=Date.now(),iam=a.iam;
  const valid=await db.prepare(`SELECT 1 FROM assignments grant_row
    JOIN organization_nodes c ON c.id=grant_row.company_id AND c.status='active'
    WHERE grant_row.id=? AND grant_row.company_id=? AND grant_row.user_id=? AND grant_row.role=?
    AND (grant_row.scope_kind='holding' OR (grant_row.scope_kind='branch' AND grant_row.scope_node_id=?) OR (grant_row.scope_kind='department' AND grant_row.scope_node_id=?))
    AND (grant_row.scope_kind='holding' OR EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=grant_row.scope_node_id AND n.company_id=grant_row.company_id AND n.status='active' AND (n.branch_id IS NULL OR EXISTS(SELECT 1 FROM organization_nodes b WHERE b.id=n.branch_id AND b.company_id=grant_row.company_id AND b.status='active'))))
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
  if(!valid)throw new ApiError(403,"EXECUTIVE_ACCESS_CHANGED","Your access changed. Reload and select an active work role.");
}
export async function executiveEligible(db:SqlDatabase,a:WorkContext) {
  if(a.role==="holding_owner" && hasPermission(a,"company.owner"))return true;
  return !!await db.prepare(`SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
    JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id
    WHERE p.company_id=? AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL
    AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')
    AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
    AND l.active=1 AND l.level_number IN (1,2) LIMIT 1`).bind(a.companyId,a.userId).first();
}
export async function canAuthor(db:SqlDatabase,a:WorkContext) {
  return hasPermission(a,"executive.access") && hasPermission(a,"executive.report.create") && await executiveEligible(db,a);
}
export async function requireAuthor(db:SqlDatabase,a:WorkContext,permission="executive.report.create") {
  await currentAuthority(db,a);
  if(!await canAuthor(db,a)||!hasPermission(a,permission))throw new ApiError(403,"EXECUTIVE_AUTHOR_REQUIRED","Executive authoring requires an active Level 1 or Level 2 engagement and a reviewed Executive access grant. Holding owners are also eligible.");
}
export async function executiveVisible(db:SqlDatabase,a:WorkContext) {
  return await canAuthor(db,a)||!!await db.prepare("SELECT 1 FROM executive_report_shares WHERE company_id=? AND recipient_id=? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?) LIMIT 1").bind(a.companyId,a.userId,Date.now()).first();
}
