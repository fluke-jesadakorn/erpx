import "server-only";
import { inventoryLedgerProjectionSql } from './inventory-read-projections';
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  inventory_ledger: inventoryLedgerProjectionSql,
} as const;
