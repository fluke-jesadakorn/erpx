import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {
  nativePeoplePayrollReadAll,
  PEOPLE_PAYROLL_RD_EXPENSE_PAYMENT_DRAFT_EXPENSE_ID,
} from './native-people-payroll-read';

/** Owner-native read: SQL is planned by the People Payroll native read port
 * (`RD_EXPENSE_PAYMENT_DRAFT_EXPENSE_ID`) and executed on the caller's open
 * unit; this owner carries no SQL for this read. Canonical form shared by the
 * expense-payment-cancellation, expense-command-policy and document-page
 * consumers. */
export async function expensePaymentDraftExpenseId(db:SqlDatabase,id:string,companyId:string){
  return (await nativePeoplePayrollReadAll<{expense_id:string}>(PEOPLE_PAYROLL_RD_EXPENSE_PAYMENT_DRAFT_EXPENSE_ID,[id,companyId],db)).results[0]??null;
}

export function documentPageExpensePaymentSource(db:SqlDatabase,paymentId:string,companyId:string){
  return expensePaymentDraftExpenseId(db,paymentId,companyId);
}
