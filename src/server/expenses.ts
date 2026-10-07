/** Compatibility entry for the Expense slice; persistence lives in native adapters. */
export { expenseDraftSchema, expenseUpdateSchema, saveExpense, expenseDecisionSchema, expensePostSchema, expenseVersionSchema, canAccountExpenses, canPostExpenses, listExpenses, getExpense, canEditExpense, canReviewExpense, expenseDetail, expenseOptions, transitionExpense, revokeExpenseAttachment, postExpense, prepareExpensePosting, expenseReversalSchema, reverseExpense } from "./modules/people_payroll/adapters/expenses";
export type { ExpenseRow } from "./modules/people_payroll/adapters/expenses";
