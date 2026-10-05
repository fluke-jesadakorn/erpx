import type { SqlDatabase } from "./runtime/types";
import { isTransactionalSqlDatabase } from "./runtime/types";
import { financialConstraintError } from "./commands";
import { ApiError } from "./http";
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
