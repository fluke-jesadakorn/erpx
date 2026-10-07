import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { mayEditExpense } from "../domain/expense-policy";
import type { ExpenseCommandPort, ExpenseMutation, ExpensePaymentInput, ExpenseRecord } from "./expense-commands";

export interface ExpenseRequirementsPorts {
  employment(row: ExpenseRecord): Promise<{ startDate: string; endDate: string | null; activeLevel: boolean } | { unavailable: string }>;
  hasReceipt(row: ExpenseRecord): Promise<boolean>;
}
export function createExpenseRequirements(ports: ExpenseRequirementsPorts) {
  return async (row: ExpenseRecord) => {
    const missing: string[] = [], employment = await ports.employment(row);
    if ("unavailable" in employment) missing.push(employment.unavailable);
    else {
      if (!employment.activeLevel) missing.push("Ask an administrator to set your active staff level in People.");
      if (row.expense_date && (String(row.expense_date) < employment.startDate || (employment.endDate && String(row.expense_date) > employment.endDate)))
        missing.push("The expense date must fall within your engagement dates.");
    }
    if (!row.expense_date) missing.push("Add the expense date.");
    if (String(row.merchant ?? "").length < 2) missing.push("Add the merchant.");
    if (String(row.receipt_reference ?? "").length < 2) missing.push("Add the receipt number.");
    if (String(row.business_purpose ?? "").length < 5) missing.push("Explain the business purpose.");
    if (!row.amount_minor || !row.currency) missing.push("Add a positive amount and currency.");
    if (!row.category_account_id) missing.push("Choose an expense category.");
    if (!await ports.hasReceipt(row)) missing.push(row.request_kind === "payment" ? "Attach the payment request’s supporting document before submitting." : "Attach the receipt before submitting.");
    return missing;
  };
}

export interface ExpensePaymentPreparationPorts extends ExpenseCommandPort {
  payment(actor: WorkContext, id: string): Promise<{ expense: ExpenseRecord; draft: ExpenseRecord }>;
  money(value: string): number;
  evidence(actor: WorkContext, id: string, version: number, key: string): Promise<unknown>;
  prepareSaving(actor: WorkContext, key: string, id: string, input: ExpensePaymentInput, expense: ExpenseRecord, amount: number, evidence: unknown): Promise<ExpenseMutation>;
}
export function createSaveExpensePayment(ports: ExpensePaymentPreparationPorts) {
  return async (actor: WorkContext, id: string, input: ExpensePaymentInput, key: string, expenseId: string) => {
    const operation = "expense.payment.save", prior = await ports.replay(actor, key, operation, id, input);
    if (prior) return prior;
    const { expense, draft } = await ports.payment(actor, id);
    if (expense.id !== expenseId) throw ports.error({ status: 404, code: "PAYMENT_DRAFT_NOT_FOUND", message: "Payment evidence does not belong to this claim." });
    const amount = ports.money(input.amount);
    if (amount <= 0 || amount > Number(expense.outstanding_minor)) throw ports.error({ status: 422, code: "PAYMENT_AMOUNT_INVALID", message: "Enter an amount within the outstanding claim balance." });
    const evidence = await ports.evidence(actor, id, Number(draft.version), key);
    return ports.commit(actor, key, operation, input, id, input.expectedVersion + 1, await ports.prepareSaving(actor, key, id, input, expense, amount, evidence));
  };
}

export type ExpenseEvidenceLink = { id: string; state: string; set_id: string };
export interface ExpensePaymentSlipRemovalPorts extends ExpenseCommandPort {
  readOnly(): boolean;
  payment(actor: WorkContext, id: string): Promise<{ expense: ExpenseRecord }>;
  link(actor: WorkContext, id: string, attachmentId: string): Promise<ExpenseEvidenceLink | null>;
  prepareRemoval(actor: WorkContext, key: string, id: string, attachmentId: string, expectedVersion: number, expense: ExpenseRecord, link: ExpenseEvidenceLink | null): Promise<ExpenseMutation>;
}
export function createRevokePaymentSlip(ports: ExpensePaymentSlipRemovalPorts) {
  return async (actor: WorkContext, expenseId: string, id: string, attachmentId: string, expectedVersion: number, key: string) => {
    const input = { attachmentId, expectedVersion }, operation = "expense.payment.slip.remove", prior = await ports.replay(actor, key, operation, id, input);
    if (prior) return prior;
    if (ports.readOnly()) throw ports.error({ status: 503, code: "ATTACHMENTS_READ_ONLY", message: "Payment-slip changes are disabled during attachment rollback." });
    const { expense } = await ports.payment(actor, id);
    if (expense.id !== expenseId) throw ports.error({ status: 404, code: "PAYMENT_DRAFT_NOT_FOUND", message: "Payment evidence does not belong to this claim." });
    const link = await ports.link(actor, id, attachmentId);
    if (link?.state === "bound") throw ports.error({ status: 409, code: "BOUND_ATTACHMENT_IMMUTABLE", message: "Bound payment evidence cannot be removed. Add a correction instead." });
    return ports.commit(actor, key, operation, input, id, expectedVersion + 1, await ports.prepareRemoval(actor, key, id, attachmentId, expectedVersion, expense, link));
  };
}

export interface ExpenseReceiptRemovalPorts extends ExpenseCommandPort {
  readOnly(): boolean;
  expense(actor: WorkContext, id: string): Promise<ExpenseRecord>;
  /** Undefined denotes legacy evidence; canonical evidence requires a draft link. */
  link(actor: WorkContext, id: string, attachmentId: string): Promise<ExpenseEvidenceLink | null | undefined>;
  draftLink(actor: WorkContext, id: string, attachmentId: string): Promise<Pick<ExpenseEvidenceLink, "id" | "set_id"> | null>;
  prepareRemoval(actor: WorkContext, key: string, id: string, attachmentId: string, expectedVersion: number, expense: ExpenseRecord, link: Pick<ExpenseEvidenceLink, "id" | "set_id"> | null): Promise<ExpenseMutation>;
}
export function createRevokeExpenseAttachment(ports: ExpenseReceiptRemovalPorts) {
  return async (actor: WorkContext, id: string, attachmentId: string, expectedVersion: number, key: string) => {
    const input = { attachmentId, expectedVersion }, operation = "expense.receipt.remove", prior = await ports.replay(actor, key, operation, id, input);
    if (prior) return prior;
    if (ports.readOnly()) throw ports.error({ status: 503, code: "ATTACHMENTS_READ_ONLY", message: "Receipt changes are disabled during attachment rollback." });
    const expense = await ports.expense(actor, id);
    if (!mayEditExpense(actor, expense)) throw ports.error({ status: 403, code: "EXPENSE_LOCKED", message: "Receipt changes require an editable claim." });
    const link = await ports.link(actor, id, attachmentId);
    if (link !== undefined) {
      if (!link) throw ports.error({ status: 404, code: "ATTACHMENT_NOT_FOUND", message: "Receipt not found." });
      if (link.state === "bound") throw ports.error({ status: 409, code: "BOUND_ATTACHMENT_IMMUTABLE", message: "Bound expense evidence cannot be removed. Add a correction instead." });
      if (link.state !== "draft") throw ports.error({ status: 404, code: "ATTACHMENT_NOT_FOUND", message: "Receipt not found." });
    }
    const canonical = await ports.draftLink(actor, id, attachmentId);
    return ports.commit(actor, key, operation, input, id, expectedVersion + 1, await ports.prepareRemoval(actor, key, id, attachmentId, expectedVersion, expense, canonical), { companyId: String(expense.company_id), branchId: expense.branch_id ? String(expense.branch_id) : null, departmentId: expense.department_id ? String(expense.department_id) : null });
  };
}
