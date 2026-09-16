import { samePerson } from "@/lib/iam";
import type { SqlDatabase } from "./runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import { assertion, executeCommand, replayCommand } from "./commands";
import {
  approvalView,
  closeApprovals,
  openApproval,
  prepareDecision,
} from "./approvals";
import { activeStaff, notification, staffGuard } from "./staff-policy";
import {
  expenseRoute,
  expenseRoleUsers,
  expenseAuthoritySnapshot,
} from "./expense-routing";
import { expenseRequirements } from "./expense-drafts";
import { getExpense, type ExpenseRow } from "./expenses";
import { holdingResources } from "./runtime";
import { decimalToMinor } from "./finance";
import { ApiError } from "./http";
import { resolveFlow } from "./flow-routing";

export const expenseAccountingSchema = z.object({
  expectedVersion: z.number().int().positive(),
  categoryAccountId: z.string().min(1),
  tax: z.string().regex(/^\d+(\.\d{1,2})?$/),
  entryDate: z.iso.date(),
  periodId: z.string().min(1),
  note: z.string().trim().max(2000).default(""),
});
const scope = (row: ExpenseRow) => ({
  company_id: String(row.company_id),
  branch_id: row.branch_id ? String(row.branch_id) : null,
  department_id: row.department_id ? String(row.department_id) : null,
});
const eventScope = (row: ExpenseRow) => ({
  companyId: String(row.company_id),
  branchId: row.branch_id ? String(row.branch_id) : null,
  departmentId: row.department_id ? String(row.department_id) : null,
});
export async function expensePayers(db: SqlDatabase, row: ExpenseRow) {
  const history = await db
    .prepare(
      "SELECT DISTINCT s.approver_user_id FROM approval_steps s JOIN approval_instances i ON i.id=s.instance_id WHERE i.record_type='expense' AND i.record_id=? AND i.company_id=? AND s.state='approved' AND i.cycle=(SELECT MAX(cycle) FROM approval_instances WHERE record_type='expense' AND record_id=?)",
    )
    .bind(row.id, row.company_id, row.id)
    .all<{ approver_user_id: string }>();
  const accounting = JSON.parse(String(row.accounting_json ?? "{}")) as {
    preparedBy?: string;
  };
  const forbidden = new Set([
    row.claimant_user_id,
    row.created_by,
    row.prepared_by,
    accounting.preparedBy,
    ...history.results.map((r) => r.approver_user_id),
  ]);
  return (await expenseRoleUsers(db, scope(row), "treasury")).filter(
    (id) => !forbidden.has(id),
  );
}
export async function expenseWorkflowFields(
  db: SqlDatabase,
  actor: WorkContext,
  row: ExpenseRow,
): Promise<ExpenseRow> {
  if (row.cancelled_at)
    return {
      ...row,
      status: "cancelled",
      workflow_stage: "cancelled",
      can_decide: 0,
    };
  const view = await approvalView(db, "expense", String(row.id), actor);
  const candidates = view.eligibleApproverIds ?? [];
  const payers =
    row.workflow_stage === "payment" ? await expensePayers(db, row) : [];
  const flow = await resolveFlow(db, actor, "expense", String(row.id));
  return {
    ...row,
    spending_template_version:JSON.parse(view.steps[0]?.route_snapshot_json??"{}").templateVersion??null,
    can_post: row.workflow_stage === "gl_posting" && flow.canAct ? 1 : 0,
    can_decide: view.canDecide && flow.canAct ? 1 : 0,
    current_approver_id: view.canDecide
      ? actor.userId
      : (view.currentApprover?.approver_user_id ?? null),
    current_approver_name: flow.current?.primary?.name ?? null,
    approval_stage: view.instance?.state ?? null,
    approval_blocked_reason:
      flow.blocker ?? view.blockedReason ??
      (row.workflow_stage === "payment" && !payers.length
        ? "An independent Treasury operator is required to record payment."
        : null),
    eligible_approvers_json: JSON.stringify(candidates),
    can_pay: payers.includes(actor.userId) && flow.canAct ? 1 : 0,
  };
}
export async function saveExpenseAccounting(
  actor: WorkContext,
  id: string,
  input: z.infer<typeof expenseAccountingSchema>,
  key: string,
) {
  const { write } = await holdingResources(actor.holdingId),
    prior = await replayCommand(
      write,
      actor,
      key,
      "expense.accounting.save",
      id,
      input,
    );
  if (prior) return prior;
  const authoritySnapshot = await expenseAuthoritySnapshot(
    write,
    actor.companyId,
  );
  const row = await getExpense(actor, id),
    view = await approvalView(write, "expense", id, actor);
  if (
    row.workflow_version !== 2 ||
    row.workflow_stage !== "accounts_review" ||
    !view.canDecide
  )
    throw new ApiError(
      403,
      "ACCOUNTS_REVIEW_REQUIRED",
      "Only the current independent Accounts queue can prepare accounting details.",
    );
  const gross = Number(row.amount_minor) + Number(row.tax_minor),
    tax = decimalToMinor(input.tax),
    net = gross - tax;
  if (net <= 0 || tax < 0)
    throw new ApiError(
      422,
      "TAX_INVALID",
      "Recoverable tax must be less than the claim total.",
    );
  if (input.entryDate < String(row.expense_date))
    throw new ApiError(
      422,
      "POSTING_DATE_INVALID",
      "Posting date cannot precede the expense.",
    );
  const now = Date.now();
  return executeCommand(
    write,
    actor,
    key,
    "expense.accounting.save",
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1 },
    [
      authoritySnapshot.guard(key),
      staffGuard(write, key, actor.companyId, actor.userId),
      assertion(
        write,
        key,
        "accounting-independent",
        "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND claimant_user_id!=? AND created_by!=? AND prepared_by!=?) AND NOT EXISTS(SELECT 1 FROM approval_steps WHERE instance_id=? AND state='approved' AND approver_user_id=?)",
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
        "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND workflow_stage='accounts_review' AND cancelled_at IS NULL)",
        [id, actor.companyId, input.expectedVersion],
      ),
      assertion(
        write,
        key,
        "accounting-authority",
        "EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND id=? AND iam_role_allows(role,company_id,'expense.prepare') AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)))",
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
        "EXISTS(SELECT 1 FROM finance_accounts a JOIN commerce_settings s ON s.chart_id=a.chart_id AND s.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.status='active' AND a.category='expense' AND a.currency=?)",
        [input.categoryAccountId, actor.companyId, row.currency],
      ),
      assertion(
        write,
        key,
        "accounting-period",
        "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)",
        [input.periodId, actor.companyId, input.entryDate, input.entryDate],
      ),
      write
        .prepare(
          "UPDATE finance_expenses SET category_account_id=?,amount_minor=?,tax_minor=?,accounting_json=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          input.categoryAccountId,
          net,
          tax,
          JSON.stringify({ ...input, preparedBy: actor.userId }),
          now,
          id,
        ),
    ],
    eventScope(row),
  );
}
export async function transitionExpenseV2(
  actor: WorkContext,
  row: ExpenseRow,
  action: "submit" | "approve" | "reject" | "cancel",
  input: { expectedVersion: number; reason?: string },
  key: string,
) {
  const { write } = await holdingResources(actor.holdingId),
    id = String(row.id),
    operation = `expense.${action}`;
  const prior = await replayCommand(write, actor, key, operation, id, input);
  if (prior) return prior;
  const own = [row.claimant_user_id, row.created_by].some(id=>samePerson(actor,id)),
    now = Date.now();
  if (action !== "submit" && (input.reason?.trim().length ?? 0) < 3)
    throw new ApiError(
      422,
      "REASON_REQUIRED",
      "Record the reason for this decision.",
    );
  if (action === "submit") {
    if (
      !own ||
      !["draft", "rejected"].includes(String(row.status)) ||
      row.cancelled_at
    )
      throw new ApiError(
        403,
        "EXPENSE_OWNER_REQUIRED",
        "Only the claimant or preparer can submit an editable claim.",
      );
    const missing = await expenseRequirements(write, row);
    if (missing.length)
      throw new ApiError(422, "EXPENSE_SETUP_REQUIRED", missing.join(" "));
    const staff = await activeStaff(
      write,
      actor.companyId,
      String(row.claimant_user_id),
    );
    const route = await expenseRoute(write, actor.companyId, staff.user_id, [
      String(row.prepared_by ?? row.created_by),
    ], false, row.request_kind === "payment" ? "payment" : "expense");
    if (route.blockedReason)
      throw new ApiError(422, "EXPENSE_ROUTE_REQUIRED", route.blockedReason);
    const opened = await openApproval(
      write,
      key,
      "expense",
      id,
      "request",
      staff.user_id,
      null,
      route,
      input.expectedVersion + 1,
    );
    return executeCommand(
      write,
      actor,
      key,
      operation,
      input,
      id,
      input.expectedVersion + 1,
      {
        id,
        status: "in_review",
        stage: route.steps[0].stage,
        version: input.expectedVersion + 1,
      },
      [
        assertion(
          write,
          key,
          "expense-version",
          "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND cancelled_at IS NULL AND status IN ('draft','rejected'))",
          [id, actor.companyId, input.expectedVersion],
        ),
        staffGuard(write, key, actor.companyId, staff.user_id),
        ...opened.statements,
        assertion(
          write,
          key,
          "receipt",
          "EXISTS(SELECT 1 FROM attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL)",
          [id],
        ),
        assertion(
          write,
          key,
          "category",
          "EXISTS(SELECT 1 FROM finance_accounts a JOIN commerce_settings s ON s.chart_id=a.chart_id AND s.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.category='expense' AND a.status='active' AND a.currency=? AND s.currency=?)",
          [
            row.category_account_id,
            actor.companyId,
            row.currency,
            row.currency,
          ],
        ),
        write
          .prepare(
            "UPDATE finance_expenses SET employee_id=?,engagement_id=?,branch_id=?,department_id=?,workflow_version=2,workflow_stage=?,status='in_review',accounting_json='{}',approved_by=NULL,rejection_reason=NULL,submitted_at=?,version=version+1,updated_at=? WHERE id=?",
          )
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
      ],
      {
        companyId: actor.companyId,
        branchId: staff.branch_id,
        departmentId: staff.department_id,
      },
    );
  }
  if (action === "cancel") {
    if (
      !own ||
      !["draft", "rejected", "in_review"].includes(String(row.status)) ||
      row.cancelled_at
    )
      throw new ApiError(
        403,
        "EXPENSE_OWNER_REQUIRED",
        "Only the requester can cancel an unposted claim.",
      );
    return executeCommand(
      write,
      actor,
      key,
      operation,
      input,
      id,
      input.expectedVersion + 1,
      { id, status: "cancelled", version: input.expectedVersion + 1 },
      [
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
        write
          .prepare(
            "UPDATE finance_expenses SET workflow_stage='cancelled',status='rejected',cancelled_at=?,rejection_reason=?,version=version+1,updated_at=? WHERE id=?",
          )
          .bind(now, input.reason, now, id),
      ],
      eventScope(row),
    );
  }
  if (row.workflow_stage === "gl_posting" && row.status === "approved" && action === "reject") {
    return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status:"rejected",stage:"draft",version:input.expectedVersion+1},[
      assertion(write,key,"expense-version","EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND status='approved' AND workflow_stage='gl_posting' AND journal_id IS NULL)",[id,actor.companyId,input.expectedVersion]),
      write.prepare("UPDATE finance_expenses SET status='rejected',workflow_stage='draft',accounting_json='{}',approved_by=NULL,rejection_reason=?,version=version+1,updated_at=? WHERE id=?").bind(input.reason,now,id),
      notification(write,scope(row),String(row.created_by),"expense",id,input.expectedVersion+1,"GL reviewer returned this claim for correction."),
    ],eventScope(row));
  }
  if (row.status !== "in_review" || row.cancelled_at)
    throw new ApiError(
      409,
      "EXPENSE_NOT_PENDING",
      "This claim is not awaiting a decision.",
    );
  const decision = await prepareDecision(
    write,
    actor,
    key,
    "expense",
    id,
    action === "approve",
    input.reason!,
    input.expectedVersion,
  );
  const accounting = JSON.parse(String(row.accounting_json ?? "{}")) as {
    periodId?: string;
    entryDate?: string;
    preparedBy?: string;
  };
  if (
    action === "approve" &&
    ["accounts_review", "accounts_approval"].includes(
      String(row.workflow_stage),
    ) &&
    (!accounting.periodId || !accounting.entryDate)
  )
    throw new ApiError(
      422,
      "ACCOUNTING_REQUIRED",
      "Prepare and review the category, tax and posting details first.",
    );
  if (action === "approve" && row.workflow_stage === "accounts_approval") {
    if (accounting.preparedBy === actor.userId)
      throw new ApiError(
        403,
        "INDEPENDENT_MANAGER_REQUIRED",
        "A different Accounts manager must approve the accounting preparation.",
      );
    return executeCommand(write, actor, key, operation, input, id, input.expectedVersion + 1,
      { id, status: "approved", stage: "gl_posting", version: input.expectedVersion + 1 }, [
        assertion(write, key, "expense-version", "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND status='in_review' AND workflow_stage='accounts_approval' AND cancelled_at IS NULL)", [id, actor.companyId, input.expectedVersion]),
        ...decision.statements,
        write.prepare("UPDATE finance_expenses SET status='approved',workflow_stage='gl_posting',approved_by=?,version=version+1,updated_at=? WHERE id=?").bind(actor.userId, now, id),
        notification(write, scope(row), actor.userId, "expense", id, input.expectedVersion + 1, "Accounting approved. Review Debit / Credit and post GL before payment."),
      ], eventScope(row));
  }
  const stage =
    action === "reject"
      ? "draft"
      : row.workflow_stage === "department_approval"
        ? "accounts_review"
        : "accounts_approval";
  return executeCommand(
    write,
    actor,
    key,
    operation,
    input,
    id,
    input.expectedVersion + 1,
    {
      id,
      status: action === "reject" ? "rejected" : "in_review",
      stage,
      version: input.expectedVersion + 1,
    },
    [
      assertion(
        write,
        key,
        "expense-version",
        "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND status='in_review' AND cancelled_at IS NULL)",
        [id, actor.companyId, input.expectedVersion],
      ),
      ...decision.statements,
      write
        .prepare(
          "UPDATE finance_expenses SET status=?,workflow_stage=?,rejection_reason=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          action === "reject" ? "rejected" : "in_review",
          stage,
          action === "reject" ? input.reason : null,
          now,
          id,
        ),
    ],
    eventScope(row),
  );
}
