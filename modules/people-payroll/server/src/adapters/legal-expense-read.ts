import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {expenseFromDatabase} from '@erpx/people-payroll-server/adapters/expense-read';
import {paymentFromDatabase} from '@erpx/people-payroll-server/public/expense-payment-read';

/** Reuse all current visibility, participant and workflow checks on Legal's supplied database. */
export const readExpenseForLegal = (actor: WorkContext, id: string, database: SqlDatabase) => expenseFromDatabase(database, actor, id);
export const readExpensePaymentForLegal = (actor: WorkContext, id: string, database: SqlDatabase) => paymentFromDatabase(database, actor, id);
