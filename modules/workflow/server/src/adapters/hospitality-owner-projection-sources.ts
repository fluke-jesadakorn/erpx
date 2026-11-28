import "server-only";
import {approvalInstancesSql,approvalStepsSql,auditEventsSql} from './approval-projection-sources';
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  approval_decisions: "SELECT \"id\",\"instance_id\",\"step_id\",\"action\",\"actor_user_id\",\"previous_approver_user_id\",\"approver_user_id\",\"reason\",\"reviewed_version\",\"command_id\",\"created_at\",\"authority_json\",\"insertion_order\" FROM approval_decisions",
  approval_instances: approvalInstancesSql,
  approval_steps: approvalStepsSql,
  audit_events: auditEventsSql,
} as const;
