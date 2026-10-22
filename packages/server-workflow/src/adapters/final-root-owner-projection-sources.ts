import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  approval_instances: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"record_type\",\"record_id\",\"phase\",\"cycle\",\"requester_user_id\",\"assignee_user_id\",\"state\",\"current_step\",\"blocked_reason\",\"version\",\"created_at\",\"updated_at\",\"insertion_order\" FROM approval_instances",
  approval_steps: "SELECT \"id\",\"instance_id\",\"ordinal\",\"approver_user_id\",\"authority\",\"state\",\"approver_pool_json\",\"required_role\",\"stage\",\"route_snapshot_json\" FROM approval_steps",
} as const;
