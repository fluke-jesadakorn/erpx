import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { expenseFromDatabase } from "./expenses";
import { paymentFromDatabase } from "./expense-payments";

/** Reuse all current visibility, participant and workflow checks on Legal's supplied database. */
export const readExpenseForLegal = (actor: WorkContext, id: string, database: SqlDatabase) => expenseFromDatabase(database, actor, id);
export const readExpensePaymentForLegal = (actor: WorkContext, id: string, database: SqlDatabase) => paymentFromDatabase(database, actor, id);
