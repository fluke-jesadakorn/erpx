import {manufacturingInitialPhysicalStockCountLanding} from '@erpx/server-inventory/public/rules/manufacturing-initial-physical-stock-count';
import {initialPhysicalStockCountLanding} from "@erpx/server-inventory/public/rules/initial-physical-stock-count";
export type DomainWork = import("@erpx/feature-work_projection/lib/work-item").WorkItem;
import type {listTasks} from '@erpx/server-operations/public/staff-tasks';
import type {taskWorkState} from '@erpx/server-operations/public/staff-task-read';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {listExpenses,canEditExpense,canReviewExpense,canPostExpenses} from '@erpx/server-people_payroll/public/expense-read';
import type {
  listCounts,
  canReadCounts,
  canEditCount,
  canReviewCount,
} from "@erpx/server-inventory/public/inventory-counts";
import type {
  listTransfers,
  canEditTransfer,
  canReviewTransfer,
} from "@erpx/server-inventory/public/inventory-transfers";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import {minorToDecimal} from "@erpx/feature-finance/lib/finance-money";
type Tail<Value extends unknown[]>=Value extends[unknown,...infer Rest]?Rest:never;
export type DomainWorkPorts={listTasks(...args:Tail<Parameters<typeof listTasks>>):ReturnType<typeof listTasks>;taskWorkState(...args:Tail<Parameters<typeof taskWorkState>>):ReturnType<typeof taskWorkState>;listExpenses:typeof listExpenses;canEditExpense:typeof canEditExpense;canReviewExpense:typeof canReviewExpense;canPostExpenses:typeof canPostExpenses;listCounts:typeof listCounts;canReadCounts:typeof canReadCounts;canEditCount:typeof canEditCount;canReviewCount:typeof canReviewCount;listTransfers:typeof listTransfers;canEditTransfer:typeof canEditTransfer;canReviewTransfer:typeof canReviewTransfer;};
export function createDomainWork(ports:DomainWorkPorts){

async function getDomainWork(
  actor: WorkContext,
  source?: "expense" | "stock_count" | "transfer" | "staff_task",
): Promise<DomainWork[]> {
  const [expenses, counts, transfers] = await Promise.all([
    !source || source === "expense" ? ports.listExpenses(actor) : Promise.resolve([]),
    !actor.beta && (!source || source === "stock_count") && ports.canReadCounts(actor)
      ? ports.listCounts(actor)
      : Promise.resolve([]),
    !actor.beta && (!source || source === "transfer") && ports.canReadCounts(actor)
      ? ports.listTransfers(actor)
      : Promise.resolve([]),
  ]);
  const rows: DomainWork[] = [];
  for (const x of expenses) {
    const independent = ![
      x.created_by,
      x.prepared_by,
      x.claimant_user_id,
    ].includes(actor.userId);
    const financeScope = !actor.branchId && !actor.departmentId;
    // Missing markers retain the original disabled-mode policy. A retained
    // Expense history row cannot regain a legacy action from its role/status.
    const maySettle = x.expense_purchase_settle_existing !== 0;
    const post = maySettle && (
      x.can_post === 1 || x.workflow_version !== 2 &&
      financeScope &&
      independent &&
      ports.canPostExpenses(actor) &&
      x.status === "approved");
    const reimburse = maySettle && (
      x.workflow_version === 2
        ? x.can_pay === 1
        : financeScope &&
          independent &&
          hasPermission(actor,"finance.pay") &&
          x.status === "posted");
    const actionable =
      ports.canEditExpense(actor, x) ||
      ports.canReviewExpense(actor, x) ||
      post ||
      reimburse;
    const own = [x.created_by, x.prepared_by, x.claimant_user_id].includes(
      actor.userId,
    );
    if (
      !actionable &&
      !own &&
      !(x.approval_blocked_reason && hasPermission(actor,"iam.manage")) &&
      !(
        x.approved_by === actor.userId &&
        ["reimbursed", "cancelled"].includes(String(x.status))
      )
    )
      continue;
    const status = String(x.status),
      terminal = ["reimbursed", "cancelled"].includes(status);
    const stage = String(x.workflow_stage ?? "");
    const next =
      status === "draft" || status === "rejected"
        ? "Complete and submit claim"
        : stage === "accounts_review"
          ? "Verify documents and prepare accounting details"
          : stage === "gl_posting"
            ? "Review Debit / Credit and post GL"
          : stage === "accounts_approval"
            ? "Approve accounting for GL review"
            : stage === "payment" || reimburse
              ? "Record payment with a bank slip"
              : reviewLabel();
    function reviewLabel() {
      return status === "in_review"
        ? "Review receipt and business purpose"
        : terminal
          ? "No further action"
          : "Wait for the responsible queue";
    }
    rows.push({
      responsibleQueue:
        stage === "accounts_review"
          ? "Accounts review"
          : stage === "accounts_approval"
            ? "Accounts manager"
            : stage === "payment" || reimburse
              ? "Treasury"
              : status === "in_review"
                ? "Department approval"
                : own
                  ? "Claimant"
                  : "Expense processing",
      nextAction: next,
      relationships: [
        ...(own ? ["requested" as const] : []),
        ...(ports.canReviewExpense(actor, x) || post ? ["approval" as const] : []),
        ...(reimburse ? ["assigned" as const] : []),
      ],
      id: String(x.id),
      type: "expense",
      actionKind: ports.canReviewExpense(actor,x) && stage !== "accounts_review" ? "approval" : "work",
      title: `${x.legal_name} · ${x.merchant || "Expense draft"}`,
      detail: `${actionable ? (status === "in_review" ? "Review receipt and business purpose" : post ? "Post approved obligation" : reimburse ? "Record reimbursement" : "Complete and submit claim") : terminal ? "Expense completed" : "Waiting for expense processing"} · ${x.currency} ${minorToDecimal(Number(x.outstanding_minor ?? Number(x.amount_minor) + Number(x.tax_minor)))}`,
      href: `/expenses/${x.id}`,
      status,
      updatedAt: Number(x.updated_at),
      canAct: actionable,
      category: "operational",
      queue: actionable ? "todo" : terminal ? "completed" : "waiting",
    });
  }
  for (const x of counts) {
    const editable = ports.canEditCount(actor, x),
      review = ports.canReviewCount(actor, x),
      own = [x.created_by, x.prepared_by].some(id=>samePerson(actor,id));
    if (
      !editable &&
      !review &&
      !own &&
      !(x.approved_by === actor.userId && x.status === "posted")
    )
      continue;
    const status = String(x.status),
      terminal = ["posted", "cancelled"].includes(status);
    rows.push({
      id: String(x.id),
      type: "stock_count",
      actionKind: review && status === "submitted" ? "approval" : "work",
      responsibleQueue: review ? "Finance review" : "Warehouse",
      relationships: [
        ...(own ? ["requested" as const] : []),
        ...(review ? ["approval" as const] : []),
        ...(editable ? ["assigned" as const] : []),
      ],
      title: `${x.number} · ${x.warehouse_name}`,
      detail: review
        ? "Review physical count and post inventory adjustment"
        : editable
          ? "Complete and submit physical count"
          : terminal
            ? "Stock count completed"
            : "Waiting for independent finance review",
      href: x.manufacturing_initial_physical_source===1||String(x.manufacturing_initial_physical_source)==="true"?`/${manufacturingInitialPhysicalStockCountLanding}/${encodeURIComponent(String(x.id))}`:x.initial_physical_source===1||String(x.initial_physical_source)==="true"?`/${initialPhysicalStockCountLanding}/${encodeURIComponent(String(x.id))}`:`/stock-counts/${x.id}`,
      status,
      updatedAt: Number(x.updated_at),
      canAct: editable || review,
      category: "operational",
      queue: editable || review ? "todo" : terminal ? "completed" : "waiting",
    });
  }
  for (const x of transfers) {
    const editable = ports.canEditTransfer(actor, x),
      review = ports.canReviewTransfer(actor, x),
      own = [x.created_by, x.prepared_by].some(id=>samePerson(actor,id)),
      status = String(x.status),
      terminal = ["posted", "cancelled"].includes(status);
    if (
      !editable &&
      !review &&
      !own &&
      !(x.approved_by === actor.userId && terminal)
    )
      continue;
    rows.push({
      id: String(x.id),
      type: "transfer",
      actionKind: review && status === "submitted" && x.delivery_mode === "transit" ? "approval" : "work",
      responsibleQueue: review ? "Receiving warehouse" : "Source warehouse",
      relationships: [
        ...(own ? ["requested" as const] : []),
        ...(review ? ["approval" as const] : []),
        ...(editable ? ["assigned" as const] : []),
      ],
      title: `${x.number} · ${x.source_warehouse_name} → ${x.target_warehouse_name}`,
      detail: review
        ? "Confirm receiving quantities and post transfer"
        : editable
          ? "Complete and submit warehouse transfer"
          : terminal
            ? "Warehouse transfer completed"
            : "Waiting for receiving review",
      href: `/stock-transfers/${x.id}`,
      status,
      updatedAt: Number(x.updated_at),
      canAct: editable || review,
      category: "operational",
      queue: editable || review ? "todo" : terminal ? "completed" : "waiting",
    });
  }
  if (!source || source === "staff_task")
    rows.push(...(await getStaffWork(actor)));
  return rows.sort(
    (a, b) => b.updatedAt - a.updatedAt || a.id.localeCompare(b.id),
  );
}
async function getStaffWork(actor: WorkContext): Promise<DomainWork[]> {
  const rows: DomainWork[] = [];
  for (const task of await ports.listTasks(actor)) {
    const detail = await ports.taskWorkState(actor,task),
      actionable = detail.permittedActions.some((a) =>
        [
          "edit",
          "submit",
          "start",
          "save-progress",
          "complete",
          "approve",
          "reject",
        ].includes(a),
      ),
      terminal = ["completed", "cancelled"].includes(task.state);
    rows.push({
      id: task.id,
      type: "staff_task",
      actionKind: detail.permittedActions.includes("approve") ? "approval" : "work",
      dueAt: task.due_at,
      blocker: detail.blockedReason,
      responsibleQueue:
        detail.approval.currentApprover?.approver_name ?? task.assignee_name,
      relationships: [
        ...(task.creator_user_id === actor.userId
          ? ["requested" as const]
          : []),
        ...(task.assignee_user_id === actor.userId
          ? ["assigned" as const]
          : []),
        ...(detail.permittedActions.includes("approve")
          ? ["approval" as const]
          : []),
      ],
      title: task.title,
      detail: detail.blockedReason
        ? `Blocked: ${detail.blockedReason}`
        : detail.approval.currentApprover
          ? `Awaiting ${detail.approval.currentApprover.approver_name}`
          : `${task.assignee_name} · ${task.project_title ?? "Operational task"}`,
      href: `/staff-tasks/${task.id}`,
      status: task.state,
      updatedAt: task.updated_at,
      canAct: actionable,
      category: "operational",
      queue: actionable ? "todo" : terminal ? "completed" : "waiting",
    });
  }
  return rows.sort(
    (a, b) => b.updatedAt - a.updatedAt || a.id.localeCompare(b.id),
  );
}

return {getDomainWork,getStaffWork};}
