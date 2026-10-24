import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {StaffActor} from "@erpx/server-operations/public/staff-policy";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {hospitalityOwnerProjectionSources as identitySources} from "@erpx/server-identity_access/public/hospitality-owner-projection-sources";
import {hospitalityOwnerProjectionSources as workflowSources} from "@erpx/server-workflow/public/hospitality-owner-projection-sources";
import {hospitalityOwnerProjectionSources as filesSources} from "@erpx/server-files/public/hospitality-owner-projection-sources";
import {expenseApprovalReadBatch} from '@erpx/server-workflow/public/approval-planning';
import {type ApprovalView} from '@erpx/server-workflow/public/approvals';
import {attachmentsV2Readable} from "@erpx/server-files/public/attachments/feature";
import {nativeExpenseEmployment, nativeExpenseRequirements} from "@erpx/server-people_payroll/adapters/expense-drafts";
import type {ExpenseRow} from "@erpx/server-people_payroll/adapters/expenses";

export interface ExpenseBatchFacts {
  auditReader(actor:StaffActor,row:ExpenseRow):Promise<boolean>;
  participant(actor:StaffActor,row:ExpenseRow):Promise<boolean>;
  approval(actor:StaffActor,id:string):Promise<ApprovalView>;
  requirements(row:ExpenseRow):Promise<string[]>;
}
/** Facts are bounded to this candidate list and actor; commands never use this snapshot. */
export async function expenseBatchFacts(db:SqlDatabase, actor:WorkContext, rows:readonly ExpenseRow[]):Promise<ExpenseBatchFacts> {
  if (rows.length>250 || rows.some(row=>row.company_id!==actor.companyId)) throw new Error("EXPENSE_BATCH_SCOPE_MISMATCH");
  const ids=rows.map(row=>String(row.id)), requested=new Set(ids), now=Date.now();
  const readerIds=rows.filter(row=>row.workflow_version===2 && ![row.claimant_user_id,row.created_by,row.approved_by].includes(actor.userId)).map(row=>String(row.id));
  const readerEncoded=JSON.stringify(readerIds);
  const receiptRows=rows.filter(row=>row.workflow_version===2 && ["draft","rejected"].includes(String(row.status)) && !row.cancelled_at);
  const receiptEncoded=JSON.stringify(receiptRows.map(row=>String(row.id)));
  const workflowIds=rows.filter(row=>!receiptRows.includes(row) && !row.cancelled_at || readerIds.includes(String(row.id))).map(row=>String(row.id));
  const [audit, participants, receipts, approval] = await Promise.all([
    readerIds.length ? db.prepare(`SELECT x.id FROM finance_expenses x WHERE x.company_id=? AND x.id IN(SELECT value FROM erpx_json_each(?)) AND EXISTS(SELECT 1 FROM (${identitySources.assignments}) assignments WHERE company_id=? AND id=? AND iam_role_allows(role,company_id,'expense.audit.read') AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=x.branch_id) OR (scope_kind='department' AND scope_node_id=x.department_id)))`).bind(actor.companyId,readerEncoded,actor.companyId,actor.assignmentId,now,now).all<{id:string}>() : Promise.resolve({results:[]}),
    readerIds.length ? db.prepare(`SELECT DISTINCT i.record_id id FROM (${workflowSources.approval_decisions}) d JOIN (${workflowSources.approval_instances}) i ON i.id=d.instance_id WHERE i.record_type='expense' AND i.record_id IN(SELECT value FROM erpx_json_each(?)) AND i.company_id=? AND d.actor_user_id=? AND d.action IN('approve','reject')`).bind(readerEncoded,actor.companyId,actor.userId).all<{id:string}>() : Promise.resolve({results:[]}),
    receiptRows.length ? db.prepare(attachmentsV2Readable()
      ? `SELECT DISTINCT a.owner_id id FROM (${filesSources.attachments}) a JOIN (${filesSources.attachment_links}) l ON l.attachment_id=a.id JOIN (${filesSources.attachment_sets}) s ON s.id=l.set_id WHERE a.owner_type='expense_claim' AND a.owner_id IN(SELECT value FROM erpx_json_each(?)) AND a.revoked_at IS NULL AND a.source_state='available' AND l.state='bound' AND s.target_type='expense_claim' AND s.target_id=a.owner_id`
      : `SELECT DISTINCT owner_id id FROM (${filesSources.attachments}) attachments WHERE owner_type='expense_claim' AND owner_id IN(SELECT value FROM erpx_json_each(?)) AND revoked_at IS NULL`).bind(receiptEncoded).all<{id:string}>() : Promise.resolve({results:[]}),
    expenseApprovalReadBatch(db,actor,workflowIds),
  ]);
  const auditors=new Set(audit.results.map(row=>row.id)), participantsById=new Set(participants.results.map(row=>row.id)), receiptIds=new Set(receipts.results.map(row=>row.id));
  const readEmployment=nativeExpenseEmployment(db);
  // Coalesce facts only for this list; receipt/date rules still run for each row.
  const employment=new Map<string, ReturnType<typeof readEmployment>>();
  const requirements=nativeExpenseRequirements(db,row=>Promise.resolve(receiptIds.has(String(row.id))),row=>{
    const key=JSON.stringify([row.company_id,row.claimant_user_id]);
    let result=employment.get(key);
    if(!result){result=readEmployment(row);employment.set(key,result);}
    return result;
  });
  function guard(context:StaffActor,id:string) {
    if(context.companyId!==actor.companyId || context.userId!==actor.userId || context.assignmentId!==actor.assignmentId || context.role!==actor.role || !requested.has(id)) throw new Error("EXPENSE_BATCH_SCOPE_MISMATCH");
  }
  return {
    async auditReader(context,row){guard(context,String(row.id));return auditors.has(String(row.id));},
    async participant(context,row){guard(context,String(row.id));return participantsById.has(String(row.id));},
    approval(context,id){guard(context,id);return approval(id);},
    requirements(row){guard(actor,String(row.id));return requirements(row);},
  };
}
