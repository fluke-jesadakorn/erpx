import { mayEditExpense, mayReviewExpense, type ExpenseActorFacts, type ExpenseRecordFacts } from "@erpx/server-people_payroll/domain/expense-policy";

export function expensePermittedActions(actor: ExpenseActorFacts, expense: ExpenseRecordFacts) {
  return [
    ...(mayEditExpense(actor, expense) ? ["save", "submit"] : []),
    ...(mayReviewExpense(actor, expense) ? ["approve", "reject"] : []),
    ...(expense.can_post === 1 ? ["post", "reject"] : []),
    ...([expense.claimant_user_id, expense.created_by].includes(actor.userId) && ["draft", "rejected", "in_review"].includes(String(expense.status)) ? ["cancel"] : []),
  ];
}
