import{createNativeFlowReader}from '@erpx/server-workflow/public/flow-reader';
import{requireExpensePurchasedHistory,expenseDocumentFlow}from '@erpx/server-people_payroll/public/expense-purchases';
const expenseFlow=createNativeFlowReader('expense',{requireHistory:(db,actor)=>requireExpensePurchasedHistory(db,actor),decorate:(view,db,actor)=>expenseDocumentFlow(db,actor,view)});

import { hospitalityOwnerProjectionSources as workflowProjectionSources } from "@erpx/server-workflow/public/hospitality-owner-projection-sources";
import { createExpensePayers, createExpenseWorkflowFields } from "@erpx/server-people_payroll/application/expense-projections";

import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";

import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

import { approvalView } from '@erpx/server-workflow/public/approval-planning';

import { expenseRoleUsers } from "@erpx/server-people_payroll/adapters/expense-routing";

import {type ExpenseRow} from '@erpx/server-people_payroll/adapters/expenses';

export function nativeExpensePayers(db: SqlDatabase) {
  return createExpensePayers({
    async approvedPeople(row) { const history = await db
    .prepare(
      `SELECT DISTINCT s.approver_user_id FROM (${workflowProjectionSources.approval_steps}) s JOIN (${workflowProjectionSources.approval_instances}) i ON i.id=s.instance_id WHERE i.record_type='expense' AND i.record_id=? AND i.company_id=? AND s.state='approved' AND i.cycle=(SELECT MAX(cycle) FROM (${workflowProjectionSources.approval_instances}) approval_instances WHERE record_type='expense' AND record_id=?)`,
    )
    .bind(row.id, row.company_id, row.id)
    .all<{ approver_user_id: string }>(); return history.results.map(row => row.approver_user_id); },
    roleUsers: (scope, role) => expenseRoleUsers(db, scope, role),
  });
}
export async function expensePayers(db: SqlDatabase, row: ExpenseRow) { return nativeExpensePayers(db)(row); }
export function nativeExpenseWorkflowFields(db: SqlDatabase, readApproval?: (actor:WorkContext,id:string)=>ReturnType<typeof approvalView>) {
  return createExpenseWorkflowFields({
    approval: readApproval ?? ((actor, id) => approvalView(db, "expense", id, actor)),
    payers: row => expensePayers(db, row),
    flow: (actor, id) => expenseFlow(db, actor, id),
  });
}
export async function expenseWorkflowFields(db: SqlDatabase, actor: WorkContext, row: ExpenseRow, readApproval?: (actor:WorkContext,id:string)=>ReturnType<typeof approvalView>): Promise<ExpenseRow> {
  return nativeExpenseWorkflowFields(db, readApproval)(actor, row);
}
