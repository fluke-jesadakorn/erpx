import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  approval_decisions: "SELECT \"id\",\"instance_id\",\"step_id\",\"action\",\"actor_user_id\",\"previous_approver_user_id\",\"approver_user_id\",\"reason\",\"reviewed_version\",\"command_id\",\"created_at\",\"authority_json\",\"insertion_order\" FROM approval_decisions",
  approval_instances: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"record_type\",\"record_id\",\"phase\",\"cycle\",\"requester_user_id\",\"assignee_user_id\",\"state\",\"current_step\",\"blocked_reason\",\"version\",\"created_at\",\"updated_at\",\"insertion_order\" FROM approval_instances",
  approval_steps: "SELECT \"id\",\"instance_id\",\"ordinal\",\"approver_user_id\",\"authority\",\"state\",\"approver_pool_json\",\"required_role\",\"stage\",\"route_snapshot_json\" FROM approval_steps",
  audit_events: "SELECT \"id\",\"command_id\",\"actor_id\",\"assignment_id\",\"action\",\"record_type\",\"record_id\",\"record_version\",\"purpose\",\"created_at\",\"authority_json\" FROM audit_events",
} as const;
