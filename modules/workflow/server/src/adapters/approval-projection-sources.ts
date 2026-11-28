import "server-only";
import { selectHeadSql } from './closed-projection-sources';
/** Byte-identical closed projection fragments shared by owner join modules. Values are fixed SQL; callers retain their own scope/authority filters and supply no SQL or relation name. */
export const approvalInstancesSql = "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"record_type\",\"record_id\",\"phase\",\"cycle\",\"requester_user_id\",\"assignee_user_id\",\"state\",\"current_step\",\"blocked_reason\",\"version\",\"created_at\",\"updated_at\",\"insertion_order\" FROM approval_instances";
export const approvalStepsSql = "SELECT \"id\",\"instance_id\",\"ordinal\",\"approver_user_id\",\"authority\",\"state\",\"approver_pool_json\",\"required_role\",\"stage\",\"route_snapshot_json\" FROM approval_steps";
/** Byte-identical star-projection span shared by the by-instance step read and the Work settlement step read; emitted SQL is unchanged. */
export const approvalStepsStarSql = "SELECT * FROM approval_steps";
export const approvalStepsByInstanceSql = `${approvalStepsStarSql} WHERE instance_id=? ORDER BY ordinal`;
/** Byte-identical prefix shared by the approval-instance star reads; callers keep their own filters and bind order, emitted SQL is unchanged. */
export const approvalInstancesStarWhereSql = "SELECT * FROM approval_instances WHERE ";
/** Byte-identical generator family for approval-step ordinal point reads: callers choose the projection, the emitted SQL is unchanged. */
export const approvalStepByOrdinalSql = (columns: string): string => `${selectHeadSql}${columns} FROM approval_steps WHERE instance_id=? AND ordinal=?`;
export const auditEventsSql = "SELECT \"id\",\"command_id\",\"actor_id\",\"assignment_id\",\"action\",\"record_type\",\"record_id\",\"record_version\",\"purpose\",\"created_at\",\"authority_json\" FROM audit_events";
