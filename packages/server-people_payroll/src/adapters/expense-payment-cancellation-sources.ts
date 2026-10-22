import 'server-only';
import {hospitalityOwnerProjectionSources as identitySources} from '@erpx/server-identity_access/public/hospitality-owner-projection-sources';
import {hospitalityOwnerProjectionSources as workflowSources} from '@erpx/server-workflow/public/hospitality-owner-projection-sources';
import {recordAttachmentVersionSteps} from '@erpx/server-files/public/record-version-receipts';
/** Exact native closure, including later authorized Files-only revisions. This
 * query grants no record visibility and is used only inside owner readers. */
export const expensePaymentCancellations=`SELECT d.id,d.expense_id,d.company_id,d.version,c.actor_id,
 c.result_json::jsonb->>'reason' reason,(c.result_json::jsonb->>'version')::bigint cancelled_version
 FROM expense_payment_drafts d JOIN finance_expenses e ON e.id=d.expense_id AND e.company_id=d.company_id
 JOIN command_receipts c ON c.operation='expense.payment.cancel' AND c.record_id=d.id
  AND c.authority_json::jsonb->>'companyId'=d.company_id
  AND c.result_json::jsonb->>'id'=d.id AND c.result_json::jsonb->>'expenseId'=d.expense_id
  AND c.result_json::jsonb->>'status'='cancelled' AND c.result_json::jsonb->>'paymentNotExecuted'='true'
 WHERE d.status='cancelled' AND d.payment_id IS NULL
  AND c.result_json::jsonb->>'version'~'^[1-9][0-9]*$'
  AND c.result_json::jsonb->>'expenseVersion'~'^[1-9][0-9]*$'
  AND (c.result_json::jsonb->>'expenseVersion')::bigint<=e.version
  AND length(trim(c.result_json::jsonb->>'reason')) BETWEEN 3 AND 2000
  AND EXISTS(SELECT 1 FROM command_receipts prepared WHERE prepared.operation='expense.payment.prepare'
   AND prepared.record_id=e.id AND prepared.authority_json::jsonb->>'companyId'=e.company_id
   AND prepared.actor_id=d.created_by AND d.id='expense-payment-'||prepared.command_id
   AND prepared.result_json::jsonb->>'id'=e.id AND prepared.result_json::jsonb->>'draftId'=d.id)
  AND NOT EXISTS(SELECT 1 FROM command_receipts confirmed WHERE confirmed.operation='expense.payment.confirm' AND confirmed.record_id=d.id)
  AND EXISTS(WITH RECURSIVE versions(version) AS(SELECT (c.result_json::jsonb->>'version')::bigint
   UNION SELECT binding.next_version FROM versions JOIN (${recordAttachmentVersionSteps}) binding
    ON binding.previous_version=versions.version AND binding.company_id=d.company_id
    AND binding.target_type='expense_payment' AND binding.target_id=d.id WHERE binding.next_version<=d.version)
   SELECT 1 FROM versions WHERE version=d.version)`;

export const expensePreparedPaymentSources=`SELECT d.id,d.expense_id,d.company_id FROM expense_payment_drafts d JOIN command_receipts prepared
 ON prepared.operation='expense.payment.prepare' AND prepared.record_id=d.expense_id
 AND prepared.actor_id=d.created_by AND prepared.authority_json::jsonb->>'companyId'=d.company_id
 AND d.id='expense-payment-'||prepared.command_id AND prepared.result_json::jsonb->>'id'=d.expense_id
 AND prepared.result_json::jsonb->>'draftId'=d.id`;
/** Current Treasury permission/scope and original independence, correlated to
 * one owner Expense row. The two bind arguments are assignmentId and userId. */
export function expensePaymentTreasurySql(alias:'x'|'e'){
 if(alias!=='x'&&alias!=='e')throw Error('EXPENSE_OWNER_ALIAS_REQUIRED');
 return `EXISTS(SELECT 1 FROM (${identitySources.assignments}) a WHERE a.id=? AND a.user_id=? AND a.company_id=${alias}.company_id
  AND iam_role_allows(a.role,a.company_id,'expense.pay') AND iam_role_allows(a.role,a.company_id,'workflow.treasury')
  AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
  AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=${alias}.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=${alias}.department_id))
  AND NOT COALESCE(iam_same_person(${alias}.claimant_user_id,a.user_id,a.company_id),false) AND NOT COALESCE(iam_same_person(${alias}.created_by,a.user_id,a.company_id),false)
  AND NOT COALESCE(iam_same_person(${alias}.prepared_by,a.user_id,a.company_id),false) AND NOT COALESCE(iam_same_person(COALESCE(${alias}.accounting_json::jsonb->>'preparedBy',''),a.user_id,a.company_id),false)
  AND NOT EXISTS(SELECT 1 FROM (${workflowSources.approval_instances}) i JOIN (${workflowSources.approval_steps}) s ON s.instance_id=i.id
   WHERE i.record_type='expense' AND i.record_id=${alias}.id AND i.company_id=${alias}.company_id AND s.state='approved' AND iam_same_person(s.approver_user_id,a.user_id,a.company_id)
    AND i.cycle=(SELECT MAX(cycle) FROM (${workflowSources.approval_instances}) WHERE record_type='expense' AND record_id=${alias}.id AND company_id=${alias}.company_id)))`;
}
