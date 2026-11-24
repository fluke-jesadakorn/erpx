import "server-only";
/** Byte-identical closed projection fragments shared by owner join modules. Values are fixed SQL; callers retain their own scope/authority filters and supply no SQL or relation name. */
export const approvalInstancesSql = "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"record_type\",\"record_id\",\"phase\",\"cycle\",\"requester_user_id\",\"assignee_user_id\",\"state\",\"current_step\",\"blocked_reason\",\"version\",\"created_at\",\"updated_at\",\"insertion_order\" FROM approval_instances";
export const approvalStepsSql = "SELECT \"id\",\"instance_id\",\"ordinal\",\"approver_user_id\",\"authority\",\"state\",\"approver_pool_json\",\"required_role\",\"stage\",\"route_snapshot_json\" FROM approval_steps";
export const approvalStepsByInstanceSql = "SELECT * FROM approval_steps WHERE instance_id=? ORDER BY ordinal";
export const auditEventsSql = "SELECT \"id\",\"command_id\",\"actor_id\",\"assignment_id\",\"action\",\"record_type\",\"record_id\",\"record_version\",\"purpose\",\"created_at\",\"authority_json\" FROM audit_events";
