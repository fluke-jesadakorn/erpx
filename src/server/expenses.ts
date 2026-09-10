import type { SqlDatabase } from "./runtime/types";
import { requireBusinessRole } from "./responsibilities";
import { hasBusinessRole } from "@/lib/responsibilities";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { assertion, executeCommand, replayCommand } from "./commands";
import { canManagePeople } from "./people-directory";
import { minorToDecimal, prepareJournal } from "./finance";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
import { requestRoute, staffGuard } from "./staff-policy";
import {
  approvalView,
  openApproval,
  prepareDecision,
  closeApprovals,
  participantSql,
} from "./approvals";
import { expenseRequirements } from "./expense-drafts";
import { expenseWorkflowFields, transitionExpenseV2 } from "./expense-workflow";
export {
  expenseDraftSchema,
  expenseUpdateSchema,
  saveExpenseDraft as saveExpense,
} from "./expense-drafts";

export type ExpenseRow = Record<string, string | number | null>;
export const expenseDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});
export const expensePostSchema = z.object({
  expectedVersion: z.number().int().positive(),
  periodId: z.string().min(1),
  entryDate: z.iso.date(),
});
export const expenseVersionSchema = z.object({
  expectedVersion: z.number().int().positive(),
});
const financeRoles = [
  "finance_preparer",
  "finance_approver",
  "treasury",
  "auditor",
  "holding_owner",
];
export function canAccountExpenses(actor: WorkContext) {
  return hasBusinessRole(actor,financeRoles);
}
export function canPostExpenses(actor: WorkContext) {
  return hasBusinessRole(actor,["holding_owner", "finance_approver"]);
}

function visibility(actor: WorkContext) {
  return {
    sql: `x.company_id=? AND (COALESCE(x.claimant_user_id,p.user_id)=? OR x.created_by=? OR x.approved_by=? OR ${participantSql("expense", "x")} OR
      ((?::text IS NULL OR x.branch_id=?) AND (?::text IS NULL OR x.department_id=?) AND ?=1 AND x.status IN ('in_review','approved','posted','reimbursed')) OR
      (x.workflow_version=2 AND x.status IN ('in_review','posted','reimbursed') AND EXISTS(SELECT 1 FROM assignments access WHERE access.user_id=? AND access.company_id=x.company_id AND access.role IN ('finance_preparer','finance_approver','treasury','holding_owner','hr_admin') AND access.revoked_at IS NULL AND access.valid_from<=? AND access.valid_until>? AND (access.scope_kind='holding' OR (access.scope_kind='branch' AND access.scope_node_id=x.branch_id) OR (access.scope_kind='department' AND access.scope_node_id=x.department_id)))))`,
    args: [
      actor.companyId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.userId,
      actor.branchId,
      actor.branchId,
      actor.departmentId,
      actor.departmentId,
      canAccountExpenses(actor) ? 1 : 0,
      actor.userId,
      Date.now(),
      Date.now(),
    ],
  };
}
async function currentExpenseReader(
  db: SqlDatabase,
  actor: WorkContext,
  row: ExpenseRow,
) {
  if (
    row.workflow_version !== 2 ||
    [row.claimant_user_id, row.created_by, row.approved_by].includes(
      actor.userId,
    )
  )
    return true;
  if (
    await db
      .prepare(
        "SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND role IN ('holding_owner','hr_admin','finance_preparer','finance_approver','treasury','auditor') AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?))",
      )
      .bind(
        actor.companyId,
        actor.userId,
        Date.now(),
        Date.now(),
        row.branch_id,
        row.department_id,
      )
      .first()
  )
    return true;
  if (
    await db
      .prepare(
        "SELECT 1 FROM approval_decisions d JOIN approval_instances i ON i.id=d.instance_id WHERE i.record_type='expense' AND i.record_id=? AND i.company_id=? AND d.actor_user_id=? AND d.action IN ('approve','reject')",
      )
      .bind(row.id, actor.companyId, actor.userId)
      .first()
  )
    return true;
  return (await approvalView(db, "expense", String(row.id), actor)).canDecide;
}
async function approvalFields(
  write: SqlDatabase,
  actor: WorkContext,
  row: ExpenseRow,
): Promise<ExpenseRow> {
  if (
    row.workflow_version === 2 &&
    ["draft", "rejected"].includes(String(row.status)) &&
    !row.cancelled_at
  ) {
    const missing = await expenseRequirements(write, row);
    return {
      ...row,
      setup_requirements_json: JSON.stringify(missing),
      approval_blocked_reason: missing[0] ?? null,
      can_decide: 0,
    };
  }
  if (row.workflow_version === 2)
    return expenseWorkflowFields(write, actor, row);
  const view = await approvalView(write, "expense", String(row.id), actor);
  const blocked =
    ["draft", "rejected"].includes(String(row.status)) && !row.cancelled_at
      ? (
          await requestRoute(
            write,
            actor.companyId,
            String(row.claimant_user_id ?? row.created_by),
            "expense",
            [String(row.prepared_by ?? row.created_by)],
          )
        ).blockedReason
      : view.blockedReason;
  return {
    ...row,
    status: row.cancelled_at ? "cancelled" : row.status,
    current_approver_id: view.currentApprover?.approver_user_id ?? null,
    current_approver_name: view.currentApprover?.approver_name ?? null,
    approval_stage: view.instance?.state ?? null,
    approval_blocked_reason: blocked,
    can_decide: view.canDecide ? 1 : 0,
  };
}
const expenseScope = (row: ExpenseRow) => ({
  companyId: String(row.company_id),
  branchId: row.branch_id ? String(row.branch_id) : null,
  departmentId: row.department_id ? String(row.department_id) : null,
});
const expenseSelect = `SELECT x.*,COALESCE(p.legal_name,x.claimant_name,'Company member') legal_name,COALESCE(x.claimant_user_id,p.user_id) claimant_user_id,a.name category_name,i.invoice_number,i.outstanding_minor
  FROM finance_expenses x LEFT JOIN hr_people p ON p.id=x.employee_id
  LEFT JOIN finance_accounts a ON a.id=x.category_account_id LEFT JOIN finance_invoices i ON i.id=x.invoice_id`;
export async function listExpenses(actor: WorkContext): Promise<ExpenseRow[]> {
  const { write } = await holdingResources(actor.holdingId),
    scope = visibility(actor);
  const legacy = await write
    .prepare(
      "SELECT id FROM finance_expenses WHERE company_id=? AND workflow_version=1 AND status='in_review' AND cancelled_at IS NULL LIMIT 250",
    )
    .bind(actor.companyId)
    .all<{ id: string }>();
  for (const row of legacy.results)
    await approvalView(write, "expense", row.id, actor);
  const rows = await write
    .prepare(
      `${expenseSelect} WHERE ${scope.sql} ORDER BY x.created_at DESC LIMIT 250`,
    )
    .bind(...scope.args)
    .all<ExpenseRow>();
  const visible = await Promise.all(
    rows.results.map(async (row) =>
      (await currentExpenseReader(write, actor, row))
        ? approvalFields(write, actor, row)
        : null,
    ),
  );
  return visible.filter((row): row is ExpenseRow => row !== null);
}
export async function getExpense(
  actor: WorkContext,
  id: string,
): Promise<ExpenseRow> {
  const { write } = await holdingResources(actor.holdingId),
    scope = visibility(actor);
  if (
    await write
      .prepare(
        "SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND workflow_version=1",
      )
      .bind(id, actor.companyId)
      .first()
  )
    await approvalView(write, "expense", id, actor);
  const row = await write
    .prepare(`${expenseSelect} WHERE x.id=? AND ${scope.sql}`)
    .bind(id, ...scope.args)
    .first<ExpenseRow>();
  if (!row || !(await currentExpenseReader(write, actor, row)))
    throw new ApiError(
      404,
      "EXPENSE_NOT_FOUND",
      "Expense claim not found in your workspace.",
    );
  return approvalFields(write, actor, row);
}
export function canEditExpense(actor: WorkContext, row: ExpenseRow) {
  return (
    !row.cancelled_at &&
    ["draft", "rejected"].includes(String(row.status)) &&
    (row.created_by === actor.userId || row.claimant_user_id === actor.userId)
  );
}
export function canReviewExpense(actor: WorkContext, row: ExpenseRow) {
  return (
    row.can_decide === 1 &&
    (row.workflow_version === 2 || row.current_approver_id === actor.userId) &&
    row.status === "in_review" &&
    ![row.created_by, row.prepared_by, row.claimant_user_id].includes(
      actor.userId,
    )
  );
}
export async function expenseDetail(actor: WorkContext, id: string) {
  const expense = await getExpense(actor, id),
    { write } = await holdingResources(actor.holdingId);
  const [attachments, history, payments, decisions] = await Promise.all([
    write
      .prepare(
        "SELECT id,original_name,media_type,bytes FROM attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL ORDER BY created_at",
      )
      .bind(id)
      .all<ExpenseRow>(),
    write
      .prepare(
        "SELECT a.id,a.action,a.created_at,COALESCE(p.legal_name,a.actor_id) actor_name FROM audit_events a LEFT JOIN hr_people p ON p.user_id=a.actor_id AND p.company_id=? AND p.merged_into_id IS NULL WHERE a.record_id IN (?,?) ORDER BY a.created_at DESC LIMIT 50",
      )
      .bind(actor.companyId, id, expense.invoice_id)
      .all<ExpenseRow>(),
    write
      .prepare(
        "SELECT p.id,p.payment_number,p.payment_date,p.currency,a.amount_minor FROM finance_allocations a JOIN finance_payments p ON p.id=a.payment_id WHERE a.invoice_id=? ORDER BY p.payment_date",
      )
      .bind(expense.invoice_id)
      .all<ExpenseRow>(),
    write
      .prepare(
        "SELECT d.*,COALESCE(p.legal_name,d.actor_id) actor_name FROM expense_decisions d LEFT JOIN hr_people p ON p.user_id=d.actor_id AND p.company_id=? AND p.merged_into_id IS NULL WHERE d.expense_id=? ORDER BY d.created_at DESC",
      )
      .bind(actor.companyId, id)
      .all<ExpenseRow>(),
  ]);
  const [paymentDrafts, paymentAttachments] = await Promise.all([
    write
      .prepare(
        "SELECT * FROM expense_payment_drafts WHERE expense_id=? AND company_id=? ORDER BY created_at",
      )
      .bind(id, actor.companyId)
      .all<ExpenseRow>(),
    write
      .prepare(
        "SELECT a.id,a.owner_id,a.original_name,a.media_type,a.bytes FROM attachments a JOIN expense_payment_drafts d ON d.id=a.owner_id WHERE a.owner_type='expense_payment' AND d.expense_id=? AND d.company_id=? AND a.revoked_at IS NULL ORDER BY a.created_at",
      )
      .bind(id, actor.companyId)
      .all<ExpenseRow>(),
  ]);
  return {
    paymentDrafts: paymentDrafts.results,
    paymentAttachments: paymentAttachments.results,
    approval: await approvalView(write, "expense", id, actor),
    permittedActions: [
      ...(canEditExpense(actor, expense)
        ? ["save", "submit"]
        : []),
      ...(canReviewExpense(actor, expense) ? ["approve", "reject"] : []),
      ...([expense.claimant_user_id, expense.created_by].includes(
        actor.userId,
      ) && ["draft", "rejected", "in_review"].includes(String(expense.status))
        ? ["cancel"]
        : []),
    ],
    expense,
    attachments: attachments.results,
    history: history.results,
    payments: payments.results,
    decisions: decisions.results,
  };
}
export async function expenseOptions(actor: WorkContext) {
  const { write } = await holdingResources(actor.holdingId);
  const [people, accounts, periods] = await Promise.all([
    write
      .prepare(
        `SELECT p.id,p.legal_name,p.user_id,e.branch_id FROM hr_people p JOIN hr_engagements e ON e.id=(SELECT id FROM hr_engagements WHERE person_id=p.id AND status='active' ORDER BY start_date DESC LIMIT 1)
      WHERE p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND (p.user_id=? OR ((?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?) AND ?=1)) ORDER BY p.legal_name`,
      )
      .bind(
        actor.companyId,
        actor.userId,
        actor.branchId,
        actor.branchId,
        actor.departmentId,
        actor.departmentId,
        canManagePeople(actor) ? 1 : 0,
      )
      .all<ExpenseRow>(),
    write
      .prepare(
        "SELECT a.id,a.name,a.code,a.currency FROM finance_accounts a JOIN commerce_settings s ON s.chart_id=a.chart_id AND s.company_id=a.company_id WHERE a.company_id=? AND a.category='expense' AND a.status='active' ORDER BY CASE WHEN a.code='5100' THEN 0 ELSE 1 END,a.code",
      )
      .bind(actor.companyId)
      .all<ExpenseRow>(),
    write
      .prepare(
        "SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' AND EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND role IN ('holding_owner','finance_preparer','finance_approver') AND revoked_at IS NULL AND valid_from<=? AND valid_until>?) ORDER BY start_date DESC",
      )
      .bind(
        actor.companyId,
        actor.companyId,
        actor.userId,
        Date.now(),
        Date.now(),
      )
      .all<ExpenseRow>(),
  ]);
  return {
    people: people.results,
    accounts: accounts.results,
    periods: periods.results,
    selfUserId: actor.userId,
  };
}
function independentClaimant(
  write: SqlDatabase,
  key: string,
  id: string,
  actor: WorkContext,
) {
  return assertion(
    write,
    key,
    "independent-claimant",
    "EXISTS(SELECT 1 FROM finance_expenses x JOIN hr_people p ON p.id=x.employee_id WHERE x.id=? AND p.user_id IS DISTINCT FROM ?)",
    [id, actor.userId],
  );
}
function versionGuard(
  write: SqlDatabase,
  key: string,
  id: string,
  actor: WorkContext,
  version: number,
  states: string[],
) {
  return assertion(
    write,
    key,
    "expense-version",
    `EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND cancelled_at IS NULL AND status IN (${states.map(() => "?").join(",")}))`,
    [id, actor.companyId, version, ...states],
  );
}
export async function transitionExpense(
  actor: WorkContext,
  id: string,
  action: "submit" | "approve" | "reject" | "cancel",
  input: { expectedVersion: number; reason?: string },
  key: string,
) {
  const { write } = await holdingResources(actor.holdingId),
    operation = `expense.${action}`;
  const prior = await replayCommand(write, actor, key, operation, id, input);
  if (prior) return prior;
  const row = await getExpense(actor, id),
    review = action === "approve" || action === "reject";
  if (row.workflow_version === 2 || ["submit"].includes(action))
    return transitionExpenseV2(actor, row, action, input, key);
  if (
    !review &&
    !(action === "cancel"
      ? [row.claimant_user_id, row.created_by].includes(actor.userId) &&
        ["draft", "rejected", "in_review"].includes(String(row.status))
      : canEditExpense(actor, row))
  )
    throw new ApiError(
      403,
      "EXPENSE_OWNER_REQUIRED",
      "Only the requester or preparer can submit or cancel this claim.",
    );
  if (action !== "submit" && !input.reason?.trim())
    throw new ApiError(
      422,
      "REVIEW_REASON_REQUIRED",
      "Record the reason for this decision.",
    );
  if (review && !canReviewExpense(actor, row))
    throw new ApiError(
      403,
      "CURRENT_APPROVER_REQUIRED",
      "Only the independent named approver can review this claim.",
    );
  const decision = review
    ? await prepareDecision(
        write,
        actor,
        key,
        "expense",
        id,
        action === "approve",
        input.reason ?? "",
        input.expectedVersion,
      )
    : null;
  const route =
    action === "submit"
      ? await requestRoute(
          write,
          actor.companyId,
          String(row.claimant_user_id),
          "expense",
          [String(row.prepared_by ?? row.created_by)],
        )
      : null;
  const opened = route
    ? await openApproval(
        write,
        key,
        "expense",
        id,
        "request",
        String(row.claimant_user_id),
        null,
        route,
        input.expectedVersion + 1,
      )
    : null;
  const now = Date.now(),
    status =
      action === "submit"
        ? "in_review"
        : action === "approve"
          ? "approved"
          : "rejected";
  const steps = [
    versionGuard(
      write,
      key,
      id,
      actor,
      input.expectedVersion,
      review
        ? ["in_review"]
        : action === "cancel"
          ? ["draft", "rejected", "in_review"]
          : ["draft", "rejected"],
    ),
    ...(opened?.statements ?? []),
    ...(decision?.statements ?? []),
    ...(action === "cancel"
      ? closeApprovals(
          write,
          key,
          "expense",
          id,
          actor.userId,
          "cancel",
          input.reason ?? "",
          input.expectedVersion,
        )
      : []),
    ...(action === "submit"
      ? [
          staffGuard(write, key, actor.companyId, String(row.claimant_user_id)),
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
            "required-inputs",
            "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND length(business_purpose)>=5 AND length(receipt_reference)>=2)",
            [id],
          ),
        ]
      : []),
    ...(route
      ? [
          write
            .prepare(
              "UPDATE finance_expenses SET branch_id=?,department_id=? WHERE id=?",
            )
            .bind(route.scope.branch_id, route.scope.department_id, id),
        ]
      : []),
    write
      .prepare(
        "UPDATE finance_expenses SET status=?,submitted_at=CASE WHEN ?='submit' THEN ? ELSE submitted_at END,approved_by=CASE WHEN ?='approve' THEN ? WHEN ?='submit' THEN NULL ELSE approved_by END,rejection_reason=?,cancelled_at=CASE WHEN ?='cancel' THEN ? ELSE cancelled_at END,version=version+1,updated_at=? WHERE id=?",
      )
      .bind(
        status,
        action,
        now,
        action,
        actor.userId,
        action,
        action === "reject" || action === "cancel"
          ? (input.reason ?? null)
          : null,
        action,
        now,
        now,
        id,
      ),
    ...(review ? [independentClaimant(write, key, id, actor)] : []),
    ...(action !== "submit"
      ? [
          write
            .prepare(
              "INSERT INTO expense_decisions(id,expense_id,command_id,action,actor_id,reviewed_version,reason,created_at) VALUES(?,?,?,?,?,?,?,?)",
            )
            .bind(
              crypto.randomUUID(),
              id,
              key,
              action,
              actor.userId,
              input.expectedVersion,
              input.reason!,
              now,
            ),
          write
            .prepare(
              "INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)",
            )
            .bind(
              crypto.randomUUID(),
              key,
              id,
              operation,
              actor.userId,
              actor.assignmentId,
              input.expectedVersion,
              await sha256(JSON.stringify(input)),
              now,
            ),
        ]
      : []),
  ];
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
      status: action === "cancel" ? "cancelled" : status,
      version: input.expectedVersion + 1,
    },
    steps,
    expenseScope(route ? { ...row, ...route.scope } : row),
  );
}
export async function revokeExpenseAttachment(
  actor: WorkContext,
  id: string,
  attachmentId: string,
  expectedVersion: number,
  key: string,
) {
  const { write } = await holdingResources(actor.holdingId),
    input = { attachmentId, expectedVersion };
  const prior = await replayCommand(
    write,
    actor,
    key,
    "expense.receipt.remove",
    id,
    input,
  );
  if (prior) return prior;
  const expense = await getExpense(actor, id);
  if (!canEditExpense(actor, expense))
    throw new ApiError(
      403,
      "EXPENSE_LOCKED",
      "Receipt changes require an editable claim.",
    );
  return executeCommand(
    write,
    actor,
    key,
    "expense.receipt.remove",
    input,
    id,
    expectedVersion + 1,
    { id, status: expense.status, version: expectedVersion + 1 },
    [
      versionGuard(write, key, id, actor, expectedVersion, [
        "draft",
        "rejected",
      ]),
      assertion(
        write,
        key,
        "receipt",
        "EXISTS(SELECT 1 FROM attachments WHERE id=? AND owner_id=? AND owner_type='expense_claim' AND revoked_at IS NULL)",
        [attachmentId, id],
      ),
      write
        .prepare("UPDATE attachments SET revoked_at=? WHERE id=?")
        .bind(Date.now(), attachmentId),
      write
        .prepare(
          "UPDATE finance_expenses SET version=version+1,updated_at=? WHERE id=?",
        )
        .bind(Date.now(), id),
    ],
    expenseScope(expense),
  );
}
export async function postExpense(
  actor: WorkContext,
  id: string,
  input: z.infer<typeof expensePostSchema>,
  key: string,
) {
  actor = requireBusinessRole(actor, ["holding_owner", "finance_approver"]);
  if (!canPostExpenses(actor) || actor.branchId || actor.departmentId)
    throw new ApiError(
      403,
      "FINANCE_APPROVER_REQUIRED",
      "A holding-level finance approver must post expense obligations.",
    );
  const { write } = await holdingResources(actor.holdingId);
  const prior = await replayCommand(
    write,
    actor,
    key,
    "expense.post",
    id,
    input,
  );
  if (prior) return prior;
  const claim = await getExpense(actor, id);
  if (claim.workflow_version === 2)
    throw new ApiError(
      409,
      "EXPENSE_STAGE_REQUIRED",
      "Use the Accounts manager approval on this claim to approve and post together.",
    );
  const prepared = await prepareExpensePosting(write, actor, claim, input, key);
  return executeCommand(
    write,
    actor,
    key,
    "expense.post",
    input,
    id,
    input.expectedVersion + 1,
    prepared.result,
    prepared.steps,
    expenseScope(claim),
  );
}
export async function prepareExpensePosting(
  write: SqlDatabase,
  actor: WorkContext,
  claim: ExpenseRow,
  input: z.infer<typeof expensePostSchema>,
  key: string,
) {
  const id = String(claim.id);
  if (
    [claim.created_by, claim.prepared_by, claim.claimant_user_id].includes(
      actor.userId,
    )
  )
    throw new ApiError(
      403,
      "SELF_POST_FORBIDDEN",
      "A different finance approver must post this expense.",
    );
  if (input.entryDate < String(claim.expense_date))
    throw new ApiError(
      422,
      "POSTING_DATE_INVALID",
      "Posting date cannot precede the expense.",
    );
  const config = await write
    .prepare(
      "SELECT s.* FROM commerce_settings s JOIN finance_accounts a ON a.chart_id=s.chart_id WHERE s.company_id=? AND a.id=? AND s.currency=?",
    )
    .bind(actor.companyId, claim.category_account_id, claim.currency)
    .first<ExpenseRow>();
  if (!config)
    throw new ApiError(
      422,
      "EXPENSE_MAPPING_REQUIRED",
      "Configure matching expense, tax and payable accounts for this currency.",
    );
  const total = Number(claim.amount_minor) + Number(claim.tax_minor),
    invoice = `expense-invoice-${id}`,
    payee = `expense-payee-${claim.employee_id}`;
  const lines = [
    {
      accountId: String(claim.category_account_id),
      debit: minorToDecimal(Number(claim.amount_minor)),
      credit: "0",
    },
    {
      accountId: String(config.payable_account_id),
      debit: "0",
      credit: minorToDecimal(total),
    },
  ];
  if (Number(claim.tax_minor))
    lines.push({
      accountId: String(config.input_tax_account_id),
      debit: minorToDecimal(Number(claim.tax_minor)),
      credit: "0",
    });
  const journal = await prepareJournal(
    write,
    actor,
    {
      chartId: String(config.chart_id),
      periodId: input.periodId,
      entryDate: input.entryDate,
      currency: String(claim.currency),
      memo: `Expense reimbursement · ${claim.legal_name} · ${claim.merchant}`,
      sourceType: "expense_claim",
      sourceId: id,
      isOpening: false,
      lines,
    },
    key,
    "posted",
  );
  const now = Date.now();
  return {
    result: {
      id,
      status: "posted",
      version: input.expectedVersion + 1,
      invoiceId: invoice,
      journalId: journal.result.id,
    },
    steps: [
      versionGuard(
        write,
        key,
        id,
        actor,
        input.expectedVersion,
        claim.workflow_version === 2 ? ["in_review"] : ["approved"],
      ),
      independentClaimant(write, key, id, actor),
      assertion(
        write,
        key,
        "expense-account",
        "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND category='expense')",
        [claim.category_account_id, actor.companyId],
      ),
      assertion(
        write,
        key,
        "receipt",
        "EXISTS(SELECT 1 FROM attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL)",
        [id],
      ),
      write
        .prepare(
          "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,person_id,created_at,updated_at) VALUES(?,?,'employee',?,?,?,0,?,?,?) ON CONFLICT(company_id,person_id) WHERE person_id IS NOT NULL DO UPDATE SET name=excluded.name",
        )
        .bind(
          payee,
          actor.companyId,
          `EMP-${claim.employee_id}`,
          claim.legal_name,
          claim.currency,
          claim.employee_id,
          now,
          now,
        ),
      assertion(
        write,
        key,
        "payee",
        "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND person_id=? AND currency=? AND status='active')",
        [payee, actor.companyId, claim.employee_id, claim.currency],
      ),
      ...journal.steps,
      write
        .prepare(
          "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,status,journal_id,created_by,approved_by,posted_at,created_at,updated_at) VALUES(?,?,?,'payable',?,?,?,?,?,?,?,?,?,?,?,'expense_claim',?,'posted',?,?,?,?,?,?)",
        )
        .bind(
          invoice,
          actor.companyId,
          payee,
          `EXP-${(await sha256(id)).slice(0, 24).toUpperCase()}`,
          input.entryDate,
          input.entryDate,
          claim.currency,
          claim.amount_minor,
          claim.tax_minor,
          total,
          total,
          claim.category_account_id,
          config.payable_account_id,
          config.input_tax_account_id,
          id,
          journal.result.id,
          claim.prepared_by ?? claim.created_by,
          claim.approved_by,
          now,
          now,
          now,
        ),
      write
        .prepare(
          "UPDATE finance_expenses SET status='posted',workflow_stage='payment',approved_by=CASE WHEN workflow_version=2 THEN ? ELSE approved_by END,invoice_id=?,journal_id=?,posted_at=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(actor.userId, invoice, journal.result.id, now, now, id),
    ],
  };
}

export const expenseReversalSchema = expensePostSchema.extend({
  reason: z.string().trim().min(3).max(2000),
});
export async function reverseExpense(
  actor: WorkContext,
  id: string,
  input: z.infer<typeof expenseReversalSchema>,
  key: string,
) {
  actor = requireBusinessRole(actor, ["holding_owner", "finance_approver"]);
  if (!canPostExpenses(actor) || actor.branchId || actor.departmentId)
    throw new ApiError(
      403,
      "FINANCE_APPROVER_REQUIRED",
      "A holding-level finance approver must reverse posted expenses.",
    );
  const { write } = await holdingResources(actor.holdingId),
    prior = await replayCommand(
      write,
      actor,
      key,
      "expense.reverse",
      id,
      input,
    );
  if (prior) return prior;
  const claim = await getExpense(actor, id);
  if (
    [claim.created_by, claim.prepared_by, claim.claimant_user_id].includes(
      actor.userId,
    )
  )
    throw new ApiError(
      403,
      "SELF_REVERSAL_FORBIDDEN",
      "A different finance approver must reverse this expense.",
    );
  const invoice = await write
    .prepare(
      "SELECT * FROM finance_invoices WHERE id=? AND company_id=? AND source_type='expense_claim' AND source_id=?",
    )
    .bind(claim.invoice_id, actor.companyId, id)
    .first<ExpenseRow>();
  if (
    !invoice ||
    claim.status !== "posted" ||
    invoice.status !== "posted" ||
    invoice.outstanding_minor !== invoice.total_minor
  )
    throw new ApiError(
      409,
      "EXPENSE_HAS_SETTLEMENT",
      "Only a posted expense with no recorded reimbursement can be reversed.",
    );
  if (input.entryDate < String(invoice.invoice_date))
    throw new ApiError(
      422,
      "REVERSAL_DATE_INVALID",
      "Reversal date cannot precede the original posting.",
    );
  const journal = await write
    .prepare(
      "SELECT * FROM finance_journals WHERE id=? AND company_id=? AND source_type='expense_claim' AND source_id=? AND status='posted'",
    )
    .bind(claim.journal_id, actor.companyId, id)
    .first<ExpenseRow>();
  if (!journal)
    throw new ApiError(
      409,
      "EXPENSE_JOURNAL_INVALID",
      "The expense posting is no longer available for reversal.",
    );
  const reversal = `expense-reversal-${key}`,
    now = Date.now();
  return executeCommand(
    write,
    actor,
    key,
    "expense.reverse",
    input,
    id,
    input.expectedVersion + 1,
    {
      id,
      status: "cancelled",
      version: input.expectedVersion + 1,
      reversalJournalId: reversal,
    },
    [
      versionGuard(write, key, id, actor, input.expectedVersion, ["posted"]),
      independentClaimant(write, key, id, actor),
      assertion(
        write,
        key,
        "unpaid",
        "EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND status='posted' AND version=? AND outstanding_minor=total_minor) AND NOT EXISTS(SELECT 1 FROM finance_allocations WHERE invoice_id=?)",
        [invoice.id, invoice.version, invoice.id],
      ),
      assertion(
        write,
        key,
        "no-export",
        "NOT EXISTS(SELECT 1 FROM hr_payment_batches WHERE source_type='expense' AND source_id=? AND status!='cancelled')",
        [id],
      ),
      assertion(
        write,
        key,
        "journal",
        "EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND status='posted' AND version=?)",
        [journal.id, journal.version],
      ),
      assertion(
        write,
        key,
        "period",
        "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)",
        [input.periodId, actor.companyId, input.entryDate, input.entryDate],
      ),
      write
        .prepare(
          "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'expense_reversal',?,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?",
        )
        .bind(
          reversal,
          input.periodId,
          `REV-${key}`,
          input.entryDate,
          id,
          input.reason,
          actor.userId,
          actor.userId,
          now,
          now,
          now,
          journal.id,
        ),
      write
        .prepare(
          "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?",
        )
        .bind(reversal, reversal, journal.id),
      write
        .prepare(
          "UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?",
        )
        .bind(actor.userId, now, now, journal.id),
      write
        .prepare(
          "UPDATE finance_invoices SET status='voided',outstanding_minor=0,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, invoice.id),
      write
        .prepare(
          "UPDATE finance_expenses SET status='rejected',cancelled_at=?,rejection_reason=?,reversal_journal_id=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, input.reason, reversal, now, id),
      write
        .prepare(
          "INSERT INTO expense_decisions(id,expense_id,command_id,action,actor_id,reviewed_version,reason,created_at) VALUES(?,?,?,'cancel',?,?,?,?)",
        )
        .bind(
          crypto.randomUUID(),
          id,
          key,
          actor.userId,
          input.expectedVersion,
          input.reason,
          now,
        ),
    ],
    expenseScope(claim),
  );
}
