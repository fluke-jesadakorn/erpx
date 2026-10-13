import { samePerson, type PermissionActor } from "@/lib/identity_access/iam";

export type ExpenseActorFacts = { userId: string; iam?: PermissionActor["iam"] };
export type ExpenseRecordFacts = Record<string, string | number | null>;
export type ExpenseViolation = { status: number; code: string; message: string };

export function mayEditExpense(actor: ExpenseActorFacts, row: ExpenseRecordFacts): boolean {
  return !row.cancelled_at && ["draft", "rejected"].includes(String(row.status)) &&
    (samePerson(actor, row.created_by) || samePerson(actor, row.claimant_user_id));
}

export function mayReviewExpense(actor: ExpenseActorFacts, row: ExpenseRecordFacts): boolean {
  return row.can_decide === 1 && (row.workflow_version === 2 || samePerson(actor, row.current_approver_id)) &&
    row.status === "in_review" && ![row.created_by, row.prepared_by, row.claimant_user_id].some(id => samePerson(actor, id));
}

export function expenseAccountingViolation(gross: number, tax: number, expenseDate: string, entryDate: string): ExpenseViolation | null {
  if (gross - tax <= 0 || tax < 0)
    return { status: 422, code: "TAX_INVALID", message: "Recoverable tax must be less than the claim total." };
  if (entryDate < expenseDate)
    return { status: 422, code: "POSTING_DATE_INVALID", message: "Posting date cannot precede the expense." };
  return null;
}

export function expensePostingViolation(actor: ExpenseActorFacts, row: ExpenseRecordFacts, entryDate: string): ExpenseViolation | null {
  // Preserve the exact persisted actor check used by posting; approval identity
  // aliases are independently revalidated by the transactional authority guards.
  if ([row.created_by, row.prepared_by, row.claimant_user_id].includes(actor.userId))
    return { status: 403, code: "SELF_POST_FORBIDDEN", message: "A different finance approver must post this expense." };
  if (entryDate < String(row.expense_date))
    return { status: 422, code: "POSTING_DATE_INVALID", message: "Posting date cannot precede the expense." };
  return null;
}
