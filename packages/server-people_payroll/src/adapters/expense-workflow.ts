import { PP_RD23_FINANCE_EXPENSE_DRAFT_REJECTED_GUARD } from './pp-read-tail-r23-sources';
import { ppRd29OpenFinancePeriodSql } from './pp-read-tail-r29-sources';
import { ppRd48FinanceExpenseVersionGuard } from './pp-read-tail-r48-sources';
import { ppRd49ExpenseClaimReceiptV2, ppRd49ExpenseClaimReceiptV1 } from './pp-read-tail-r49-sources';
import { ppRd51AccountingCategoryGuard } from './pp-read-tail-r51-sources';
import { nativePpStatement } from '@erpx/server-people_payroll/adapters/native-people-payroll-command';
import{nativeExpensePayers,expensePayers,nativeExpenseWorkflowFields,expenseWorkflowFields}from '@erpx/server-people_payroll/adapters/expense-workflow-read';
export{nativeExpensePayers,expensePayers,nativeExpenseWorkflowFields,expenseWorkflowFields};

import {withExpensePurchasedCommand} from "@erpx/server-people_payroll/adapters/expense-purchases";
import {peoplePackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import { hospitalityOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as workflowProjectionSources } from "@erpx/server-workflow/public/hospitality-owner-projection-sources";

import { createSaveExpenseAccounting, createTransitionExpenseV2 } from "@erpx/server-people_payroll/application/expense-commands";
import { expenseCommandPort, expenseMutation } from "@erpx/server-people_payroll/adapters/expense-command-port";
/** Native Expense adapter; preserves legacy persistence and command semantics. */
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {approvalView,closeApprovals,openApproval,prepareDecision} from '@erpx/server-workflow/public/approval-planning';
import { activeStaff, notification, staffGuard } from "@erpx/server-operations/public/staff-policy";
import { expenseRoute, expenseAuthoritySnapshot } from "@erpx/server-people_payroll/adapters/expense-routing";
import {expenseRequirements} from '@erpx/server-people_payroll/adapters/expense-requirements';
import {expenseFromDatabase} from '@erpx/server-people_payroll/adapters/expense-read';
import {type ExpenseRow} from '@erpx/server-people_payroll/adapters/expenses';
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import {decimalToMinor} from '@erpx/server-finance/public/money-primitives';

import { attachmentsV2Readable } from "@erpx/server-files/public/attachments/feature";

export const expenseAccountingSchema = z.object({
  expectedVersion: z.number().int().positive(),
  categoryAccountId: z.string().min(1),
  tax: z.string().regex(/^\d+(\.\d{1,2})?$/),
  entryDate: z.iso.date(),
  periodId: z.string().min(1),
  note: z.string().trim().max(2000).default(""),
});

const scope = (row: ExpenseRow) => ({ company_id: String(row.company_id), branch_id: row.branch_id ? String(row.branch_id) : null, department_id: row.department_id ? String(row.department_id) : null });

function originalNativeExpenseAccounting(write: SqlDatabase) {
  return createSaveExpenseAccounting({
    ...expenseCommandPort(write),
    authority: actor => expenseAuthoritySnapshot(write, actor.companyId),
    expense: (actor, id) => expenseFromDatabase(write, actor, id),
    approval: (actor, id) => approvalView(write, "expense", id, actor),
    money: decimalToMinor,
    async prepareAccounting(actor, key, id, input, row, view, capturedAuthority, tax, net) {
      const authoritySnapshot = capturedAuthority as Awaited<ReturnType<typeof expenseAuthoritySnapshot>>;
      const now = Date.now();
      return expenseMutation({ id, version: input.expectedVersion + 1 }, [
      authoritySnapshot.guard(key),
      staffGuard(write, key, actor.companyId, actor.userId),
      assertion(
        write,
        key,
        "accounting-independent",
        `EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND claimant_user_id!=? AND created_by!=? AND prepared_by!=?) AND NOT EXISTS(SELECT 1 FROM (${workflowProjectionSources.approval_steps}) approval_steps WHERE instance_id=? AND state='approved' AND approver_user_id=?)`,
        [
          id,
          actor.userId,
          actor.userId,
          actor.userId,
          view.instance!.id,
          actor.userId,
        ],
      ),
      assertion(
        write,
        key,
        "accounting-version",
        ppRd48FinanceExpenseVersionGuard("workflow_stage='accounts_review' AND cancelled_at IS NULL"),
        [id, actor.companyId, input.expectedVersion],
      ),
      assertion(
        write,
        key,
        "accounting-authority",
        `EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE company_id=? AND id=? AND iam_role_allows(role,company_id,'expense.prepare') AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)))`,
        [
          actor.companyId,
          actor.assignmentId,
          now,
          now,
          row.branch_id,
          row.department_id,
        ],
      ),
      assertion(
        write,
        key,
        "accounting-category",
        ppRd51AccountingCategoryGuard(false, 'account'),
        [input.categoryAccountId, actor.companyId, row.currency],
      ),
      assertion(
        write,
        key,
        "accounting-period",
        ppRd29OpenFinancePeriodSql(financeProjectionSources.finance_periods),
        [input.periodId, actor.companyId, input.entryDate, input.entryDate],
      ),
      nativePpStatement('pp_expense_workflow_stmt_1')
        .bind(
          input.categoryAccountId,
          net,
          tax,
          JSON.stringify({ ...input, preparedBy: actor.userId }),
          now,
          id,
        ),
    ]);
    },
  });
}
export async function saveExpenseAccounting(actor: WorkContext, id: string, input: z.infer<typeof expenseAccountingSchema>, key: string) {
  const { write } = await holdingResources(actor.holdingId);
  return withExpensePurchasedCommand(write, actor, {key,purpose:"settle-existing"}, tx => nativeExpenseAccounting(tx)(actor, id, input, key));
}
function originalNativeExpenseTransitions(write: SqlDatabase) {
  return createTransitionExpenseV2({
    ...expenseCommandPort(write),
    now: Date.now,
    requirements: row => expenseRequirements(write, row),
    staff: (actor, row) => activeStaff(write, actor.companyId, String(row.claimant_user_id)),
    async route(actor, staff, row) {
      const route = await expenseRoute(write, actor.companyId, staff.user_id, [String(row.prepared_by ?? row.created_by)], false, row.request_kind === "payment" ? "payment" : "expense");
      return { steps: route.steps, blockedReason: route.blockedReason, native: route };
    },
    open: (_actor, key, id, input, staff, route) => openApproval(write, key, "expense", id, "request", staff.user_id, null, route.native as Awaited<ReturnType<typeof expenseRoute>>, input.expectedVersion + 1),
    async decision(actor, key, id, action, input) {
      const decision = await prepareDecision(write, actor, key, "expense", id, action === "approve", input.reason!, input.expectedVersion, input.override ?? false);
      return { next: decision.next, native: decision };
    },
    async prepareSubmit(actor, key, id, input, row, staff, capturedRoute, capturedOpened, now) {
      const route = capturedRoute.native as Awaited<ReturnType<typeof expenseRoute>>;
      const opened = capturedOpened as Awaited<ReturnType<typeof openApproval>>;
      return expenseMutation({
        id,
        status: "in_review",
        stage: route.steps[0].stage,
        version: input.expectedVersion + 1,
      }, [
        assertion(
          write,
          key,
          "expense-version",
          PP_RD23_FINANCE_EXPENSE_DRAFT_REJECTED_GUARD,
          [id, actor.companyId, input.expectedVersion],
        ),
        staffGuard(write, key, actor.companyId, staff.user_id),
        ...opened.statements,
        assertion(
          write,
          key,
          "receipt",
          attachmentsV2Readable()
            ? ppRd49ExpenseClaimReceiptV2()
            : ppRd49ExpenseClaimReceiptV1(),
          [id],
        ),
        assertion(
          write,
          key,
          "category",
          ppRd51AccountingCategoryGuard(true, 'both'),
          [
            row.category_account_id,
            actor.companyId,
            row.currency,
            row.currency,
          ],
        ),
        nativePpStatement('pp_expense_workflow_stmt_2')
          .bind(
            staff.person_id,
            staff.engagement_id,
            staff.branch_id,
            staff.department_id,
            route.steps[0].stage,
            now,
            now,
            id,
          ),
      ]);
    },
    async prepareCancel(actor, key, id, input, _row, now) {

      return expenseMutation({ id, status: "cancelled", version: input.expectedVersion + 1 }, await Promise.all([
        assertion(
          write,
          key,
          "expense-version",
          "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND cancelled_at IS NULL AND status IN ('draft','rejected','in_review'))",
          [id, actor.companyId, input.expectedVersion],
        ),
        ...closeApprovals(
          write,
          key,
          "expense",
          id,
          actor.userId,
          "cancel",
          input.reason!,
          input.expectedVersion,
        ),
        nativePpStatement('pp_expense_workflow_stmt_3')
          .bind(now, input.reason, now, id),
      ]));
    },
    async prepareGlReturn(actor, key, id, input, row, now) {

      return expenseMutation({id,status:"rejected",stage:"draft",version:input.expectedVersion+1}, [
      assertion(write,key,"expense-version",ppRd48FinanceExpenseVersionGuard("status='approved' AND workflow_stage='gl_posting' AND journal_id IS NULL"),[id,actor.companyId,input.expectedVersion]),
      nativePpStatement('pp_expense_workflow_stmt_4').bind(input.reason,now,id),
      await notification(write,scope(row),String(row.created_by),"expense",id,input.expectedVersion+1,"GL reviewer returned this claim for correction."),
    ]);
    },
    async prepareAccountingApproval(actor, key, id, input, row, capturedDecision, now) {
      const decision = capturedDecision.native as Awaited<ReturnType<typeof prepareDecision>>;
      return expenseMutation({ id, status: "approved", stage: "gl_posting", version: input.expectedVersion + 1 }, [
        assertion(write, key, "expense-version", ppRd48FinanceExpenseVersionGuard("status='in_review' AND workflow_stage='accounts_approval' AND cancelled_at IS NULL"), [id, actor.companyId, input.expectedVersion]),
        ...decision.statements,
        nativePpStatement('pp_expense_workflow_stmt_5').bind(actor.userId, now, id),
        await notification(write, scope(row), actor.userId, "expense", id, input.expectedVersion + 1, "Accounting approved. Review Debit / Credit and post GL before payment."),
      ]);
    },
    async prepareDecision(actor, key, id, action, input, _row, capturedDecision, stage, now) {
      const decision = capturedDecision.native as Awaited<ReturnType<typeof prepareDecision>>;
      return expenseMutation({
      id,
      status: action === "reject" ? "rejected" : "in_review",
      stage,
      version: input.expectedVersion + 1,
    }, [
      assertion(
        write,
        key,
        "expense-version",
        ppRd48FinanceExpenseVersionGuard("status='in_review' AND cancelled_at IS NULL"),
        [id, actor.companyId, input.expectedVersion],
      ),
      ...decision.statements,
      nativePpStatement('pp_expense_workflow_stmt_6')
        .bind(
          action === "reject" ? "rejected" : "in_review",
          stage,
          action === "reject" ? input.reason : null,
          now,
          id,
        ),
    ]);
    },
  });
}
export async function transitionExpenseV2(actor: WorkContext, row: ExpenseRow, action: "submit" | "approve" | "reject" | "cancel", input: { expectedVersion: number; reason?: string; override?: boolean }, key: string) {
  const { write } = await holdingResources(actor.holdingId);
  return withExpensePurchasedCommand(write, actor, {key,purpose:action === "submit" ? "new-work" : "settle-existing"}, async tx => nativeExpenseTransitions(tx)(actor, await peoplePackageEnforcementMode() === "disabled" ? row : await expenseFromDatabase(tx, actor, String(row.id)), action, input, key));
}

export function nativeExpenseAccounting(database: SqlDatabase){return (...args:Parameters<ReturnType<typeof originalNativeExpenseAccounting>>)=>withExpensePurchasedCommand(database,args[0],{key:args[3],purpose:"settle-existing"},transaction=>originalNativeExpenseAccounting(transaction)(...args));}

export function nativeExpenseTransitions(database: SqlDatabase){return (...args:Parameters<ReturnType<typeof originalNativeExpenseTransitions>>)=>withExpensePurchasedCommand(database,args[0],{key:args[4],purpose:args[2] === "submit" ? "new-work" : "settle-existing"},transaction=>originalNativeExpenseTransitions(transaction)(...args));}
