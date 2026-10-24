import {expensePaymentCancellations,expensePreparedPaymentSources,expensePaymentTreasurySql} from '@erpx/server-people_payroll/adapters/expense-payment-cancellation-sources';

import {peoplePackageEnforcementMode,readPurchasedAccess} from "@erpx/server-commercial/public/purchase-command-boundary";
import { withExpensePurchasedRead } from "@erpx/server-people_payroll/adapters/expense-purchases";

import { hospitalityOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as workflowProjectionSources } from "@erpx/server-workflow/public/hospitality-owner-projection-sources";

import { createCurrentExpenseReader, createExpenseApprovalFields } from "@erpx/server-people_payroll/application/expense-projections";

import { createExpenseReads } from "@erpx/server-people_payroll/application/expense-reads";
import {expenseBatchFacts, type ExpenseBatchFacts} from "@erpx/server-people_payroll/adapters/expense-batch-facts";

import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { mayEditExpense, mayReviewExpense } from "@erpx/server-people_payroll/domain/expense-policy";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";

import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";

import { ApiError } from "@erpx/server-runtime/public/http";
import {departmentRoute} from '@erpx/server-organization/public/department-authority-reads';
import type {StaffActor} from "@erpx/server-operations/public/staff-policy";

import { approvalView } from '@erpx/server-workflow/public/approval-planning';
import {participantSql} from '@erpx/server-workflow/public/approval-participant-projection';
import {expenseRequirements} from '@erpx/server-people_payroll/adapters/expense-requirements';
import { expenseWorkflowFields } from '@erpx/server-people_payroll/adapters/expense-workflow-read';

export type ExpenseRow = Record<string, string | number | null>;
export function canAccountExpenses(actor: StaffActor) {
  return hasPermission(actor,"expense.read");
}
export function canPostExpenses(actor: WorkContext) {
  return hasPermission(actor,"expense.approve");
}
export function visibility(actor: StaffActor) {
  return {
    sql: `x.company_id=? AND (iam_same_person(COALESCE(x.claimant_user_id,p.user_id),?,x.company_id) OR iam_same_person(x.created_by,?,x.company_id) OR iam_same_person(x.approved_by,?,x.company_id) OR ${participantSql("expense", "x", "?", "?")} OR
      ((?::text IS NULL OR x.branch_id=?) AND (?::text IS NULL OR x.department_id=?) AND ?=1 AND x.status IN ('in_review','approved','posted','reimbursed')) OR
      (x.workflow_version=2 AND x.status IN ('in_review','approved','posted','reimbursed') AND EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) access WHERE access.id=? AND access.company_id=x.company_id AND iam_role_allows(access.role,access.company_id,'expense.review.read') AND access.revoked_at IS NULL AND iam_assignment_active(access.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(access.user_id,access.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND access.valid_from<=? AND access.valid_until>? AND (access.scope_kind='holding' OR (access.scope_kind='branch' AND access.scope_node_id=x.branch_id) OR (access.scope_kind='department' AND access.scope_node_id=x.department_id))))
      OR (x.cancelled_at IS NOT NULL AND (${expensePaymentTreasurySql('x')}) AND EXISTS(SELECT 1 FROM expense_payment_drafts duty
        WHERE duty.expense_id=x.id AND duty.company_id=x.company_id AND duty.payment_id IS NULL
        AND ((duty.status='draft' AND EXISTS(SELECT 1 FROM (${expensePreparedPaymentSources}) native WHERE native.id=duty.id AND native.company_id=duty.company_id))
         OR (duty.status='cancelled' AND EXISTS(SELECT 1 FROM (${expensePaymentCancellations}) terminal WHERE terminal.id=duty.id AND terminal.company_id=duty.company_id))))))`,
    args: [
      actor.companyId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.assignmentId,
      actor.branchId,
      actor.branchId,
      actor.departmentId,
      actor.departmentId,
      canAccountExpenses(actor) ? 1 : 0,
      actor.assignmentId,
      Date.now(),
      Date.now(),
      actor.assignmentId,
      actor.userId,
    ],
  };
}
export async function currentExpenseReader(db: SqlDatabase, actor: StaffActor, row: ExpenseRow, facts?:ExpenseBatchFacts, inspection = false) {
  return createCurrentExpenseReader({
    async auditReader(actor, row) { if(facts)return facts.auditReader(actor,row); return Boolean(await db
      .prepare(
        `SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE company_id=? AND id=? AND iam_role_allows(role,company_id,'expense.audit.read') AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?))`,
      )
      .bind(
        actor.companyId,
        actor.assignmentId,
        Date.now(),
        Date.now(),
        row.branch_id,
        row.department_id,
      )
      .first()); },
    async participant(actor, row) { if(facts)return facts.participant(actor,row); return Boolean(await db
      .prepare(
        `SELECT 1 FROM (${workflowProjectionSources.approval_decisions}) d JOIN (${workflowProjectionSources.approval_instances}) i ON i.id=d.instance_id WHERE i.record_type='expense' AND i.record_id=? AND i.company_id=? AND d.actor_user_id=? AND d.action IN ('approve','reject')`,
      )
      .bind(row.id, actor.companyId, actor.userId)
      .first()); },
    async canDecide(actor, id) { return (await (facts ? facts.approval(actor,id) : approvalView(db, "expense", id, inspection ? {...actor,inspection:true} : actor))).canDecide; },
  })(actor, row);
}
export async function approvalFields(write: SqlDatabase, actor: WorkContext, row: ExpenseRow, facts?:ExpenseBatchFacts): Promise<ExpenseRow> {
  return createExpenseApprovalFields({
    requirements: row => facts ? facts.requirements(row) : expenseRequirements(write, row),
    current: (actor, row) => expenseWorkflowFields(write, actor, row, facts?.approval),
    approval: (actor, id) => facts ? facts.approval(actor,id) : approvalView(write, "expense", id, actor),
    department: (actor, row) => departmentRoute(write, actor.companyId, String(row.claimant_user_id ?? row.created_by), [String(row.prepared_by ?? row.created_by)]),
  })(actor, row);
}
export const expenseSelect = `SELECT x.*,COALESCE(p.legal_name,x.claimant_name,'Company member') legal_name,COALESCE(x.claimant_user_id,p.user_id) claimant_user_id,a.name category_name,i.invoice_number,i.outstanding_minor
  FROM finance_expenses x LEFT JOIN hr_people p ON p.id=x.employee_id
  LEFT JOIN (${financeProjectionSources.finance_accounts}) a ON a.id=x.category_account_id LEFT JOIN (${financeProjectionSources.finance_invoices}) i ON i.id=x.invoice_id`;
export function originalNativeExpenseReads(write: SqlDatabase, actor: WorkContext, options: {batch?:boolean} = {}) {
  const scope = visibility(actor);
  const reads = createExpenseReads<ExpenseRow>({
    async legacyIds(context) {
      return (await write.prepare("SELECT id FROM finance_expenses WHERE company_id=? AND workflow_version=1 AND status='in_review' AND cancelled_at IS NULL LIMIT 250")
        .bind(context.companyId).all<{ id: string }>()).results.map(row => row.id);
    },
    async isLegacy(context, id) {
      return Boolean(await write.prepare("SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND workflow_version=1").bind(id, context.companyId).first());
    },
    async initializeApproval(context, id) { await approvalView(write, "expense", id, context); },
    async visibleRows() {
      return (await write.prepare(`${expenseSelect} WHERE ${scope.sql} ORDER BY x.created_at DESC LIMIT 250`).bind(...scope.args).all<ExpenseRow>()).results;
    },
    visibleRow: (_context, id) => write.prepare(`${expenseSelect} WHERE x.id=? AND ${scope.sql}`).bind(id, ...scope.args).first<ExpenseRow>(),
    canRead: (context, row) => currentExpenseReader(write, context, row),
    decorate: (context, row) => approvalFields(write, context, row),
    prepareBatch: options.batch === false ? undefined : async(context,rows) => {
      const facts=await expenseBatchFacts(write,context,rows);
      return {canRead:(actor,row)=>currentExpenseReader(write,actor,row,facts),decorate:(actor,row)=>approvalFields(write,actor,row,facts)};
    },
    error: violation => new ApiError(violation.status, violation.code, violation.message),
  });
  return { list: () => reads.list(actor), get: (id: string) => reads.get(actor, id) };
}
export async function listExpenses(actor: WorkContext): Promise<ExpenseRow[]> {
  const { write } = await holdingResources(actor.holdingId);
  return withExpensePurchasedRead(write, actor, tx => nativeExpenseReads(tx, actor).list());
}
export async function getExpense(actor: WorkContext, id: string, provided?: SqlDatabase): Promise<ExpenseRow> {
  const write = provided ?? (await holdingResources(actor.holdingId)).write;
  return expenseFromDatabase(write, actor, id);
}
export async function expenseFromDatabase(write: SqlDatabase, actor: WorkContext, id: string): Promise<ExpenseRow> {
  return withExpensePurchasedRead(write, actor, tx => nativeExpenseReads(tx, actor).get(id));
}
export function canEditExpense(actor: WorkContext, row: ExpenseRow) {
  return row.expense_purchase_new_work !== 0 && mayEditExpense(actor, row);
}
export function canReviewExpense(actor: WorkContext, row: ExpenseRow) {
  return row.expense_purchase_settle_existing !== 0 && mayReviewExpense(actor, row);
}
export function nativeExpenseReads(database:SqlDatabase,actor:WorkContext,options:{batch?:boolean}={}){return {
 list:()=>withExpensePurchasedRead(database,actor,async transaction=>expensePresentationRows(transaction,actor,await originalNativeExpenseReads(transaction,actor,options).list())),
 get:(id:string)=>withExpensePurchasedRead(database,actor,async transaction=>(await expensePresentationRows(transaction,actor,[await originalNativeExpenseReads(transaction,actor,options).get(id)]))[0])
};}
export async function expensePresentationRows(database:SqlDatabase,actor:WorkContext,rows:ExpenseRow[]):Promise<ExpenseRow[]>{
 const mode=await peoplePackageEnforcementMode();if(mode==='disabled')return rows;
 const access=await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'people.expenses'});
 return rows.map(row=>({...row,expense_purchase_new_work:access.newWork?1:0,expense_purchase_settle_existing:access.settleExisting?1:0,...(!access.settleExisting?{can_decide:0,can_post:0,can_pay:0}:{})}));
}
export async function expenseVisibleRecordIds(database:SqlDatabase,actor:StaffActor,ids:readonly string[],inspection=false){
 const scope=visibility(actor),permitted=new Set<string>();
 for(let start=0;start<ids.length;start+=250){
  const rows=(await database.prepare(`${expenseSelect} WHERE x.id IN(SELECT value FROM erpx_json_each(?)) AND ${scope.sql}`).bind(JSON.stringify(ids.slice(start,start+250)),...scope.args).all<ExpenseRow>()).results;
  for(const row of rows)if(await currentExpenseReader(database,actor,row,undefined,inspection))permitted.add(String(row.id));
 }
 return permitted;
}
