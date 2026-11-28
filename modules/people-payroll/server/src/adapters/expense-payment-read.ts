import { ppRd25ByIdCompany } from './pp-read-tail-r25-sources';
import { withExpensePurchasedRead } from "@erpx/people-payroll-server/adapters/expense-purchases";

import { createExpensePaymentReader } from "@erpx/people-payroll-server/application/expense-projections";

import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import "server-only";

import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import {expenseFromDatabase} from '@erpx/people-payroll-server/adapters/expense-read';
import {type ExpenseRow} from '@erpx/people-payroll-server/adapters/expenses';
import {expensePayers} from '@erpx/people-payroll-server/adapters/expense-workflow-read';
import { ApiError } from "@erpx/platform-runtime-server/public/http";

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
