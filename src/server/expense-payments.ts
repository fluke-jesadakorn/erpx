import type { SqlDatabase } from "./runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { getExpense, type ExpenseRow } from "./expenses";
import { expensePayers } from "./expense-workflow";
import { ApiError } from "./http";
import { assertion, executeCommand, replayCommand } from "./commands";
import { prepareInvoiceSettlement } from "./commerce";
import { notification, staffGuard } from "./staff-policy";
import { decimalToMinor } from "./finance";

export const expensePaymentSchema = z.object({
  expectedVersion: z.number().int().positive(),
  date: z.iso.date(),
  amount: z.string().regex(/^\d+(\.\d{1,2})?$/),
  reference: z.string().trim().min(2).max(100),
});
export async function paymentDraft(
  actor: WorkContext,
  id: string,
  editable = false,
) {
  const { write } = await holdingResources(actor.holdingId);
  const draft = await write
    .prepare("SELECT * FROM expense_payment_drafts WHERE id=? AND company_id=?")
    .bind(id, actor.companyId)
    .first<ExpenseRow>();
  if (!draft)
    throw new ApiError(
      404,
      "PAYMENT_DRAFT_NOT_FOUND",
      "Payment evidence not found in this company.",
    );
  const expense = await getExpense(actor, String(draft.expense_id));
  if (
    editable &&
    (draft.status !== "draft" ||
      expense.workflow_stage !== "payment" ||
      !(await expensePayers(write, expense)).includes(actor.userId))
  )
    throw new ApiError(
      403,
      "TREASURY_REQUIRED",
      "An independent Treasury operator must prepare this payment.",
    );
  return { draft, expense };
}
export async function createExpensePayment(
  actor: WorkContext,
  id: string,
  expectedVersion: number,
  key: string,
) {
  const { write } = await holdingResources(actor.holdingId),
    input = { expectedVersion },
    operation = "expense.payment.prepare";
  const prior = await replayCommand(write, actor, key, operation, id, input);
  if (prior) return prior;
  const row = await getExpense(actor, id);
  if (
    row.workflow_version !== 2 ||
    row.workflow_stage !== "payment" ||
    !(await expensePayers(write, row)).includes(actor.userId)
  )
    throw new ApiError(
      403,
      "TREASURY_REQUIRED",
      "Only an independent Treasury operator can record this payment.",
    );
  const draftId = `expense-payment-${key}`,
    now = Date.now();
  return executeCommand(
    write,
    actor,
    key,
    operation,
    input,
    id,
    expectedVersion,
    { id, draftId, version: expectedVersion },
    [
      ...paymentGuards(write, actor, key, row),
      assertion(
        write,
        key,
        "payment-claim-version",
        "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND status='posted' AND workflow_stage='payment')",
        [id, actor.companyId, expectedVersion],
      ),
      write
        .prepare(
          "INSERT INTO expense_payment_drafts(id,expense_id,company_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?)",
        )
        .bind(draftId, id, actor.companyId, actor.userId, now, now),
    ],
  );
}
export function paymentGuards(
  db: SqlDatabase,
  actor: WorkContext,
  key: string,
  row: ExpenseRow,
) {
  const now = Date.now();
  return [
    staffGuard(db, key, actor.companyId, actor.userId),
    assertion(
      db,
      key,
      "payment-stage",
      "EXISTS(SELECT 1 FROM finance_expenses x WHERE id=? AND company_id=? AND workflow_stage='payment' AND status='posted' AND cancelled_at IS NULL AND (branch_id IS NULL OR EXISTS(SELECT 1 FROM organization_nodes WHERE id=x.branch_id AND company_id=x.company_id AND status='active')) AND (department_id IS NULL OR EXISTS(SELECT 1 FROM organization_nodes WHERE id=x.department_id AND company_id=x.company_id AND status='active')))",
      [row.id, actor.companyId],
    ),
    assertion(
      db,
      key,
      "treasury-authority",
      "EXISTS(SELECT 1 FROM assignments WHERE id=? AND company_id=? AND iam_role_allows(role,company_id,'expense.pay') AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)))",
      [
        actor.assignmentId,
        actor.companyId,
        now,
        now,
        row.branch_id,
        row.department_id,
      ],
    ),
    assertion(
      db,
      key,
      "payment-independent",
      "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND claimant_user_id!=? AND created_by!=? AND prepared_by!=? AND COALESCE(erpx_json_extract(accounting_json,'$.preparedBy'),'')!=?) AND NOT EXISTS(SELECT 1 FROM approval_instances i JOIN approval_steps s ON s.instance_id=i.id WHERE i.record_type='expense' AND i.record_id=? AND i.company_id=? AND s.state='approved' AND s.approver_user_id=? AND i.cycle=(SELECT MAX(cycle) FROM approval_instances WHERE record_type='expense' AND record_id=?))",
      [
        row.id,
        actor.userId,
        actor.userId,
        actor.userId,
        actor.userId,
        row.id,
        actor.companyId,
        actor.userId,
        row.id,
      ],
    ),
  ];
}
export async function confirmExpensePayment(
  actor: WorkContext,
  id: string,
  input: z.infer<typeof expensePaymentSchema>,
  key: string,
  expenseId?: string,
) {
  const { write } = await holdingResources(actor.holdingId),
    operation = "expense.payment.confirm";
  const prior = await replayCommand(write, actor, key, operation, id, input);
  if (prior) return prior;
  const { expense } = await paymentDraft(actor, id, true);
  if (expenseId && expense.id !== expenseId)
    throw new ApiError(
      404,
      "PAYMENT_DRAFT_NOT_FOUND",
      "Payment evidence does not belong to this claim.",
    );
  const invoice = await write
    .prepare(
      "SELECT * FROM finance_invoices WHERE id=? AND company_id=? AND source_type='expense_claim' AND source_id=?",
    )
    .bind(expense.invoice_id, actor.companyId, expense.id)
    .first<ExpenseRow>();
  if (!invoice)
    throw new ApiError(
      409,
      "EXPENSE_PAYABLE_REQUIRED",
      "The approved expense payable is unavailable.",
    );
  if (
    !(await write
      .prepare(
        "SELECT 1 FROM attachments WHERE owner_type='expense_payment' AND owner_id=? AND media_type IN ('image/jpeg','image/png','image/webp','application/pdf') AND revoked_at IS NULL",
      )
      .bind(id)
      .first())
  )
    throw new ApiError(
      422,
      "PAYMENT_SLIP_REQUIRED",
      "Attach the real bank payment slip before confirming.",
    );
  const prepared = await prepareInvoiceSettlement(
      write,
      actor,
      invoice,
      expense,
      { ...input, expectedVersion: Number(invoice.version) },
      key,
    ),
    now = Date.now();
  return executeCommand(
    write,
    actor,
    key,
    operation,
    input,
    id,
    input.expectedVersion + 1,
    {
      ...prepared.result,
      id: String(expense.id),
      draftId: id,
      version: input.expectedVersion + 1,
    },
    [
      ...paymentGuards(write, actor, key, expense),
      assertion(
        write,
        key,
        "payment-draft-version",
        "EXISTS(SELECT 1 FROM expense_payment_drafts WHERE id=? AND company_id=? AND version=? AND status='draft')",
        [id, actor.companyId, input.expectedVersion],
      ),
      assertion(
        write,
        key,
        "payment-slip",
        "EXISTS(SELECT 1 FROM attachments WHERE owner_type='expense_payment' AND owner_id=? AND media_type IN ('image/jpeg','image/png','image/webp','application/pdf') AND revoked_at IS NULL)",
        [id],
      ),
      assertion(
        write,
        key,
        "unique-payment-slip",
        "NOT EXISTS(SELECT 1 FROM attachments current JOIN attachments used ON used.checksum_sha256=current.checksum_sha256 AND used.owner_type='expense_payment' AND used.revoked_at IS NULL JOIN expense_payment_drafts d ON d.id=used.owner_id AND d.company_id=? AND d.status='confirmed' WHERE current.owner_type='expense_payment' AND current.owner_id=? AND current.revoked_at IS NULL)",
        [actor.companyId, id],
      ),
      ...prepared.steps,
      write
        .prepare(
          "UPDATE expense_payment_drafts SET status='confirmed',payment_id=?,payment_date=?,amount_minor=?,reference=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          prepared.result.paymentId,
          input.date,
          decimalToMinor(input.amount),
          input.reference,
          now,
          id,
        ),
      notification(
        write,
        {
          company_id: actor.companyId,
          branch_id: expense.branch_id ? String(expense.branch_id) : null,
          department_id: expense.department_id
            ? String(expense.department_id)
            : null,
        },
        String(expense.claimant_user_id),
        "expense",
        String(expense.id),
        Number(expense.version) + 1,
        prepared.result.outstandingMinor === 0
          ? "Your expense has been paid in full"
          : "A partial expense payment was recorded",
      ),
    ],
    {
      companyId: actor.companyId,
      branchId: expense.branch_id ? String(expense.branch_id) : null,
      departmentId: expense.department_id
        ? String(expense.department_id)
        : null,
    },
  );
}
export async function saveExpensePayment(
  actor: WorkContext,
  id: string,
  input: z.infer<typeof expensePaymentSchema>,
  key: string,
  expenseId: string,
) {
  const { write } = await holdingResources(actor.holdingId),
    operation = "expense.payment.save";
  const prior = await replayCommand(write, actor, key, operation, id, input);
  if (prior) return prior;
  const { expense } = await paymentDraft(actor, id, true);
  if (expense.id !== expenseId)
    throw new ApiError(
      404,
      "PAYMENT_DRAFT_NOT_FOUND",
      "Payment evidence does not belong to this claim.",
    );
  const amount = decimalToMinor(input.amount);
  if (amount <= 0 || amount > Number(expense.outstanding_minor))
    throw new ApiError(
      422,
      "PAYMENT_AMOUNT_INVALID",
      "Enter an amount within the outstanding claim balance.",
    );
  return executeCommand(
    write,
    actor,
    key,
    operation,
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1 },
    [
      ...paymentGuards(write, actor, key, expense),
      assertion(
        write,
        key,
        "payment-draft-version",
        "EXISTS(SELECT 1 FROM expense_payment_drafts WHERE id=? AND version=? AND status='draft')",
        [id, input.expectedVersion],
      ),
      write
        .prepare(
          "UPDATE expense_payment_drafts SET payment_date=?,amount_minor=?,reference=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(input.date, amount, input.reference, Date.now(), id),
    ],
  );
}
export async function revokePaymentSlip(
  actor: WorkContext,
  expenseId: string,
  id: string,
  attachmentId: string,
  expectedVersion: number,
  key: string,
) {
  const { write } = await holdingResources(actor.holdingId),
    input = { attachmentId, expectedVersion },
    operation = "expense.payment.slip.remove";
  const prior = await replayCommand(write, actor, key, operation, id, input);
  if (prior) return prior;
  const { expense } = await paymentDraft(actor, id, true);
  if (expense.id !== expenseId)
    throw new ApiError(
      404,
      "PAYMENT_DRAFT_NOT_FOUND",
      "Payment evidence does not belong to this claim.",
    );
  return executeCommand(
    write,
    actor,
    key,
    operation,
    input,
    id,
    expectedVersion + 1,
    { id, version: expectedVersion + 1 },
    [
      ...paymentGuards(write, actor, key, expense),
      assertion(
        write,
        key,
        "payment-draft-version",
        "EXISTS(SELECT 1 FROM expense_payment_drafts WHERE id=? AND version=? AND status='draft')",
        [id, expectedVersion],
      ),
      assertion(
        write,
        key,
        "slip-owner",
        "EXISTS(SELECT 1 FROM attachments WHERE id=? AND owner_type='expense_payment' AND owner_id=? AND revoked_at IS NULL)",
        [attachmentId, id],
      ),
      write
        .prepare("UPDATE attachments SET revoked_at=? WHERE id=?")
        .bind(Date.now(), attachmentId),
      write
        .prepare(
          "UPDATE expense_payment_drafts SET version=version+1,updated_at=? WHERE id=?",
        )
        .bind(Date.now(), id),
    ],
  );
}
