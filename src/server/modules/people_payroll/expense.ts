/** Server-facing Expense slice. Existing callers retain their request/results. */
export { previewExpense, readExpenseAiStatus, expensePageCapabilities, requestExpenseAi, expensePostingPreview } from "./adapters/expense-queries";
export { canAccountExpenses, canPostExpenses, canEditExpense, canReviewExpense, listExpenses, getExpense, expenseDetail, expenseOptions, saveExpense, transitionExpense, postExpense, reverseExpense, revokeExpenseAttachment, expenseDraftSchema, expenseUpdateSchema, expenseDecisionSchema, expenseVersionSchema, expensePostSchema, expenseReversalSchema, type ExpenseRow } from "@/server/modules/people_payroll/adapters/expenses";
export { saveExpenseAccounting, expenseAccountingSchema } from "@/server/modules/people_payroll/adapters/expense-workflow";
export { createExpensePayment, saveExpensePayment, confirmExpensePayment, revokePaymentSlip, expensePaymentSchema } from "@/server/modules/people_payroll/adapters/expense-payments";
export { queueExpenseAi } from "@/server/modules/people_payroll/adapters/expense-ai";
