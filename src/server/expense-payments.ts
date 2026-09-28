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
import {
  attachmentsV2Enabled,
  attachmentsV2Readable,
  attachmentsV2ReadOnly,
} from "./attachments/feature";
import { prepareAttachmentBinding } from "./attachments/service";

function paymentSlipExistsSql() {
  return attachmentsV2Readable()
    ? `EXISTS(SELECT 1 FROM attachments a JOIN attachment_links l ON l.attachment_id=a.id JOIN attachment_sets s ON s.id=l.set_id
        WHERE a.owner_type='expense_payment' AND a.owner_id=?
          AND a.media_type IN ('image/jpeg','image/png','image/webp','application/pdf')
          AND a.revoked_at IS NULL AND a.source_state='available' AND l.state='bound'
          AND s.target_type='expense_payment' AND s.target_id=a.owner_id)`
    : "EXISTS(SELECT 1 FROM attachments WHERE owner_type='expense_payment' AND owner_id=? AND media_type IN ('image/jpeg','image/png','image/webp','application/pdf') AND revoked_at IS NULL)";
}

async function preparePaymentEvidence(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  expectedVersion: number,
  commandId: string,
  bindingType: string,
) {
  if (!attachmentsV2Enabled()) return null;
  const setIds = (
    await db
      .prepare(
        `SELECT id FROM attachment_sets
         WHERE company_id=? AND target_type='expense_payment' AND target_id=?
           AND component_kind='action' AND purpose_key='payment_slip'
           AND expected_target_version=? AND state='draft'
         ORDER BY created_at,id`,
      )
      .bind(actor.companyId, id, expectedVersion)
      .all<{ id: string }>()
  ).results.map((row) => row.id);
  if (!setIds.length) return null;
  return prepareAttachmentBinding(db, actor, {
    attachmentSetIds: setIds,
    bindingContract: {
      targetType: "expense_payment",
      bindingType,
      contexts: [{ purposeKey: "payment_slip", componentKind: "action", componentRef: "legacy-upload-adapter" }],
    },
    targetId: id,
    expectedTargetVersion: expectedVersion,
    commandId,
    bindingId: id,
  });
}

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
  const { expense, draft } = await paymentDraft(actor, id, true);
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
  const evidence = await preparePaymentEvidence(
    write,
    actor,
    id,
    Number(draft.version),
    key,
    "expense_payment_confirmation",
  );
  if (
    !evidence &&
    !(await write.prepare(`SELECT 1 WHERE ${paymentSlipExistsSql()}`).bind(id).first())
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
      {
        ...input,
        expectedVersion: Number(invoice.version),
        attachmentSetIds: [],
      },
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
      ...(evidence?.guards ?? []),
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
        evidence ? "?=?" : paymentSlipExistsSql(),
        evidence ? [id, id] : [id],
      ),
      assertion(
        write,
        key,
        "unique-payment-slip",
        attachmentsV2Readable()
          ? `NOT EXISTS(SELECT 1 FROM attachments current
              JOIN attachment_links current_link ON current_link.attachment_id=current.id AND current_link.state${attachmentsV2ReadOnly() ? "='bound'" : " IN ('draft','bound')"}
              JOIN attachments used ON used.checksum_sha256=current.checksum_sha256 AND used.owner_type='expense_payment' AND used.revoked_at IS NULL AND used.source_state='available'
              JOIN attachment_links used_link ON used_link.attachment_id=used.id AND used_link.state='bound'
              JOIN expense_payment_drafts d ON d.id=used.owner_id AND d.company_id=? AND d.status='confirmed'
              WHERE current.owner_type='expense_payment' AND current.owner_id=? AND current.revoked_at IS NULL AND current.source_state='available')`
          : "NOT EXISTS(SELECT 1 FROM attachments current JOIN attachments used ON used.checksum_sha256=current.checksum_sha256 AND used.owner_type='expense_payment' AND used.revoked_at IS NULL JOIN expense_payment_drafts d ON d.id=used.owner_id AND d.company_id=? AND d.status='confirmed' WHERE current.owner_type='expense_payment' AND current.owner_id=? AND current.revoked_at IS NULL)",
        [actor.companyId, id],
      ),
      ...prepared.steps,
      ...(evidence?.steps ?? []),
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
  const { expense, draft } = await paymentDraft(actor, id, true);
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
  const evidence = await preparePaymentEvidence(
    write,
    actor,
    id,
    Number(draft.version),
    key,
    "expense_payment_preparation",
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
      ...(evidence?.guards ?? []),
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
      ...(evidence?.steps ?? []),
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
  if (attachmentsV2ReadOnly())
    throw new ApiError(
      503,
      "ATTACHMENTS_READ_ONLY",
      "Payment-slip changes are disabled during attachment rollback.",
    );
  const { expense } = await paymentDraft(actor, id, true);
  if (expense.id !== expenseId)
    throw new ApiError(
      404,
      "PAYMENT_DRAFT_NOT_FOUND",
      "Payment evidence does not belong to this claim.",
    );
  const canonical = attachmentsV2Readable()
    ? await write
        .prepare(
          `SELECT l.id,l.state,l.set_id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
           WHERE l.attachment_id=? AND s.company_id=? AND s.target_type='expense_payment' AND s.target_id=?`,
        )
        .bind(attachmentId, actor.companyId, id)
        .first<{ id: string; state: string; set_id: string }>()
    : null;
  if (canonical?.state === "bound")
    throw new ApiError(
      409,
      "BOUND_ATTACHMENT_IMMUTABLE",
      "Bound payment evidence cannot be removed. Add a correction instead.",
    );
  const now = Date.now();
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
      ...(canonical
        ? [
            assertion(
              write,
              key,
              "canonical-slip-draft",
              "EXISTS(SELECT 1 FROM attachment_links WHERE id=? AND state='draft')",
              [canonical.id],
            ),
            write
              .prepare(
                "UPDATE attachment_links SET state='withdrawn',withdrawn_at=?,withdrawn_by=?,withdrawal_reason=? WHERE id=? AND state='draft'",
              )
              .bind(
                now,
                actor.userId,
                "Removed from payment preparation",
                canonical.id,
              ),
            write
              .prepare(
                "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?, 'link.withdrawn',?,?)",
              )
              .bind(
                `attachment-event-${crypto.randomUUID()}`,
                actor.companyId,
                attachmentId,
                canonical.id,
                canonical.set_id,
                actor.userId,
                JSON.stringify({ reason: "Removed from payment preparation" }),
                now,
              ),
          ]
        : []),
      write
        .prepare("UPDATE attachments SET revoked_at=? WHERE id=?")
        .bind(now, attachmentId),
      write
        .prepare(
          "UPDATE expense_payment_drafts SET version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, id),
    ],
  );
}
