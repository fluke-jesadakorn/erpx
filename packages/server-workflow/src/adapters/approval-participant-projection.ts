import {workflowFinalOwnerProjections as identityaccessFinalOwnerProjections} from "@erpx/server-identity_access/public/workflow-final-owner-projections";
const identityaccessFinalSources=identityaccessFinalOwnerProjections();
import type {ApprovalKind} from '@erpx/server-workflow/domain/approvals';
export function participantSql(
  kind: ApprovalKind,
  alias: string,
  userParameter = "?",
  activeAssignment?: string,
) {
  const roleCheck=activeAssignment ? ` AND (ast.state IN ('approved','rejected') OR ast.stage='department_approval' AND jsonb_exists(ast.route_snapshot_json::jsonb,'departmentPolicyVersion') OR ast.stage='hr_approval' AND ast.route_snapshot_json::jsonb->>'appointedPersonId' IS NOT NULL OR EXISTS(SELECT 1 ${identityaccessFinalSources.approval_participant_projection_assignments_aa_from} WHERE aa.id=${activeAssignment} AND iam_role_allows(aa.role,aa.company_id,'workflow.'||COALESCE(ast.required_role,CASE WHEN ast.authority IN ('named','department') THEN 'people_manager' ELSE ast.authority END)) AND aa.company_id=ai.company_id AND (aa.scope_kind='holding' OR aa.scope_kind='branch' AND aa.scope_node_id=ai.branch_id OR aa.scope_kind='department' AND aa.scope_node_id=ai.department_id)))` : "";
  return `EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='${kind}' AND ai.record_id=${alias}.id AND ai.company_id=${alias}.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,${userParameter},ai.company_id))${roleCheck})`;
}
