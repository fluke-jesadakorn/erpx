/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { expenseDraftSchema, expenseUpdateSchema, saveExpense, expenseDecisionSchema, expensePostSchema, expenseVersionSchema, canAccountExpenses, canPostExpenses, listExpenses, getExpense, canEditExpense, canReviewExpense, expenseDetail, expenseOptions, transitionExpense, revokeExpenseAttachment, postExpense, prepareExpensePosting, expenseReversalSchema, reverseExpense } from "@erpx/people-payroll-server/adapters/expenses";
export type { ExpenseRow } from "@erpx/people-payroll-server/adapters/expenses";
