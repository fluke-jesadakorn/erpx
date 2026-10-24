import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { isTransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import { ApiError } from "@erpx/server-runtime/public/http";
/** A posted primary journal produces its secondary journals at the deferred
 * book boundary. Materialize those rows while event capture/final line checks
 * are still deferred; making every constraint immediate first would capture a
 * child journal before its lines exist. Keep every original integrity check. */
export function financeDeferredEffectStatements(database:SqlDatabase) {
  return [database.prepare("SET CONSTRAINTS finance_book_projection IMMEDIATE"),database.prepare("SET CONSTRAINTS finance_book_projection DEFERRED"),database.prepare("SET CONSTRAINTS ALL IMMEDIATE")];
}
export async function flushFinanceDeferredEffects(database: SqlDatabase) {
  for(const statement of financeDeferredEffectStatements(database))await statement.run();
}
/** Includes deferred posting/activation constraints raised at COMMIT. */
export async function financeTransaction<T>(
  database: SqlDatabase,
  action: (tx: SqlDatabase) => Promise<T>,
): Promise<T> {
  if (!isTransactionalSqlDatabase(database))
    throw new ApiError(
      503,
      "TRANSACTION_REQUIRED",
      "This Finance workflow requires PostgreSQL transactions.",
    );
  try {
    return await database.transaction(action);
  } catch (error) {
    throw financialConstraintError(error) ?? error;
  }
}

/** Compose a scheduled owner command in its caller's active PostgreSQL transaction. */
export async function financeOwnedTransaction<T>(database: SqlDatabase, owned: SqlDatabase | undefined, action: (tx:SqlDatabase)=>Promise<T>):Promise<T>{
  if (!owned) return financeTransaction(database,action);
  const owner = database as SqlDatabase & {assertActiveTransaction?:(tx:SqlDatabase)=>void};
  if (!owner.assertActiveTransaction) throw new ApiError(503,"TRANSACTION_OWNERSHIP_REQUIRED","Scheduled drafts require an owned PostgreSQL transaction.");
  owner.assertActiveTransaction(owned);
  return action(owned);
}
