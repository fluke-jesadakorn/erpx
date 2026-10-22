import 'server-only';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function peoplePayrollFinalOwnerProjections(){return {
  "payroll_cancellation_flow_routes_f_from": "FROM (SELECT \"company_id\",\"kind\",\"record_id\",\"version\",\"defaults_revision\",\"steps_json\",\"created_at\" FROM flow_routes) f",
  "people_lifecycle_command_receipts_from": "FROM (SELECT \"command_id\",\"operation\",\"actor_id\",\"request_hash\",\"record_id\",\"result_json\",\"created_at\",\"authority_json\" FROM command_receipts) command_receipts",
  "workflow_approvals_facts_approval_steps_from": "FROM (SELECT \"id\",\"instance_id\",\"ordinal\",\"approver_user_id\",\"authority\",\"state\",\"approver_pool_json\",\"required_role\",\"stage\",\"route_snapshot_json\" FROM approval_steps) approval_steps",
  "workflow_approvals_facts_approval_instances_from": "FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"record_type\",\"record_id\",\"phase\",\"cycle\",\"requester_user_id\",\"assignee_user_id\",\"state\",\"current_step\",\"blocked_reason\",\"version\",\"created_at\",\"updated_at\",\"insertion_order\" FROM approval_instances) approval_instances"
} as const;}
