import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  operational_inventory_consumptions: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"source_kind\",\"source_id\",\"source_version\",\"warehouse_id\",\"product_id\",\"quantity_milli\",\"value_minor\",\"currency\",\"movement_date\",\"journal_id\",\"reason\",\"created_by\",\"created_at\" FROM operational_inventory_consumptions",
} as const;
