import 'server-only';
import {approvalInstancesSql,approvalStepsSql} from './approval-projection-sources';
import {flowRoutesClosedProjectionSql,commandReceiptsProjectionSql} from './closed-projection-sources';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function peoplePayrollFinalOwnerProjections(){return {
  "payroll_cancellation_flow_routes_f_from": `FROM (${flowRoutesClosedProjectionSql(',')}) f`,
  "people_lifecycle_command_receipts_from": `FROM (${commandReceiptsProjectionSql(['"command_id"','"operation"','"actor_id"','"request_hash"','"record_id"','"result_json"','"created_at"','"authority_json"'])}) command_receipts`,
  "workflow_approvals_facts_approval_steps_from": `FROM (${approvalStepsSql}) approval_steps`,
  "workflow_approvals_facts_approval_instances_from": `FROM (${approvalInstancesSql}) approval_instances`
} as const;}
