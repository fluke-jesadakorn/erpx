import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {expenseFromDatabase} from '@erpx/server-people_payroll/adapters/expense-read';
import {paymentFromDatabase} from '@erpx/server-people_payroll/public/expense-payment-read';

/** Reuse all current visibility, participant and workflow checks on Legal's supplied database. */
export const readExpenseForLegal = (actor: WorkContext, id: string, database: SqlDatabase) => expenseFromDatabase(database, actor, id);
export const readExpensePaymentForLegal = (actor: WorkContext, id: string, database: SqlDatabase) => paymentFromDatabase(database, actor, id);
