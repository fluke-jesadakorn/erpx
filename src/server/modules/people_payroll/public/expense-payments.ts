/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { expensePaymentSchema, paymentDraft, createExpensePayment, paymentGuards, confirmExpensePayment, saveExpensePayment, revokePaymentSlip } from "@/server/modules/people_payroll/adapters/expense-payments";

export {cancelExpensePayment,expensePaymentCancellationSchema,type ExpensePaymentCancellationInput,type ExpensePaymentCancellationResult} from "../adapters/expense-payment-cancellation";
