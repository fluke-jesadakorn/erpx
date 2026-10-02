import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import "server-only";
import { holdingResources } from "./runtime";
import { listTasks, taskWorkState } from "./staff-tasks";
import type { WorkContext } from "./authorization";
import {
  listExpenses,
  canEditExpense,
  canReviewExpense,
  canPostExpenses,
} from "./expenses";
import {
  listCounts,
  canReadCounts,
  canEditCount,
  canReviewCount,
} from "./inventory-counts";
import {
  listTransfers,
  canEditTransfer,
  canReviewTransfer,
} from "./inventory-transfers";
import { minorToDecimal } from "@/lib/finance-money";

export type DomainWork = import("@/lib/work-item").WorkItem;
export async function getDomainWork(
  actor: WorkContext,
  source?: "expense" | "stock_count" | "transfer" | "staff_task",
): Promise<DomainWork[]> {
  const [expenses, counts, transfers] = await Promise.all([
    !source || source === "expense" ? listExpenses(actor) : Promise.resolve([]),
    !actor.beta && (!source || source === "stock_count") && canReadCounts(actor)
      ? listCounts(actor)
      : Promise.resolve([]),
    !actor.beta && (!source || source === "transfer") && canReadCounts(actor)
      ? listTransfers(actor)
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
    const post =
      x.can_post === 1 || x.workflow_version !== 2 &&
      financeScope &&
      independent &&
      canPostExpenses(actor) &&
      x.status === "approved";
    const reimburse =
      x.workflow_version === 2
        ? x.can_pay === 1
        : financeScope &&
          independent &&
          hasPermission(actor,"finance.pay") &&
          x.status === "posted";
    const actionable =
      canEditExpense(actor, x) ||
      canReviewExpense(actor, x) ||
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
        ...(canReviewExpense(actor, x) || post ? ["approval" as const] : []),
        ...(reimburse ? ["assigned" as const] : []),
      ],
      id: String(x.id),
      type: "expense",
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
    const editable = canEditCount(actor, x),
      review = canReviewCount(actor, x),
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
      href: `/stock-counts/${x.id}`,
      status,
      updatedAt: Number(x.updated_at),
      canAct: editable || review,
      category: "operational",
      queue: editable || review ? "todo" : terminal ? "completed" : "waiting",
    });
  }
  for (const x of transfers) {
    const editable = canEditTransfer(actor, x),
      review = canReviewTransfer(actor, x),
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
export async function getStaffWork(actor: WorkContext): Promise<DomainWork[]> {
  const rows: DomainWork[] = [];
  const { write } = await holdingResources(actor.holdingId);
  for (const task of await listTasks(write, actor)) {
    const detail = await taskWorkState(write, actor, task),
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
