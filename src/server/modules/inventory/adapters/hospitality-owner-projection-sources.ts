import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  inventory_ledger: "SELECT \"id\",\"warehouse_id\",\"product_id\",\"quantity_milli\",\"value_minor\",\"created_at\",\"movement_date\",\"source_type\",\"source_id\",\"source_number\" FROM inventory_ledger",
} as const;
