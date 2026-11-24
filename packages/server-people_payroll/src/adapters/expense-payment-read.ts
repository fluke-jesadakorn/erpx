import { ppRd25ByIdCompany } from './pp-read-tail-r25-sources';
import { withExpensePurchasedRead } from "@erpx/server-people_payroll/adapters/expense-purchases";

import { createExpensePaymentReader } from "@erpx/server-people_payroll/application/expense-projections";

import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";

import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import {expenseFromDatabase} from '@erpx/server-people_payroll/adapters/expense-read';
import {type ExpenseRow} from '@erpx/server-people_payroll/adapters/expenses';
import {expensePayers} from '@erpx/server-people_payroll/adapters/expense-workflow-read';
import { ApiError } from "@erpx/server-runtime/public/http";

export async function paymentDraft(actor: WorkContext, id: string, editable = false, provided?: SqlDatabase) {
  const write = provided ?? (await holdingResources(actor.holdingId)).write;
  return paymentFromDatabase(write, actor, id, editable);
}
export function nativeExpensePaymentReader(write: SqlDatabase) {
  return createExpensePaymentReader({
    error: violation => new ApiError(violation.status, violation.code, violation.message),
    draft: (actor, id) => write.prepare(ppRd25ByIdCompany('expense_payment_drafts')).bind(id, actor.companyId).first<ExpenseRow>(),
    expense: (actor, id) => expenseFromDatabase(write, actor, id),
    payers: row => expensePayers(write, row),
  });
}
export async function paymentFromDatabase(write: SqlDatabase, actor: WorkContext, id: string, editable = false) {
  return withExpensePurchasedRead(write, actor, tx => nativeExpensePaymentReader(tx)(actor, id, editable));
}
