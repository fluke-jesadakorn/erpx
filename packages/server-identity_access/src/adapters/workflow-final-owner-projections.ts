import 'server-only';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function workflowFinalOwnerProjections(){return {
  "approval_participant_projection_assignments_aa_from": "FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) aa",
  "work_role_audit_assignments_from": "FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) assignments"
} as const;}
