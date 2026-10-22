import "server-only";
/** Closed native projections with explicit owned columns for named Finance reads.
 * The consumer retains its supplied transaction and one-statement snapshot;
 * this contract accepts no table name, predicate, SQL, or query from a caller. */
export function financeOwnedSnapshotSources(){return {
  "finance_engines_manufacturing_orders_from": "FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"warehouse_id\",\"recommendation_id\",\"specification_id\",\"specification_version\",\"planned_quantity_milli\",\"status\",\"material_cost_minor\",\"labor_cost_minor\",\"overhead_cost_minor\",\"reported_output_milli\",\"reported_scrap_milli\",\"created_by\",\"version\",\"created_at\",\"updated_at\" FROM manufacturing_orders) manufacturing_orders",
  "finance_manufacturing_orders_from": "FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"warehouse_id\",\"recommendation_id\",\"specification_id\",\"specification_version\",\"planned_quantity_milli\",\"status\",\"material_cost_minor\",\"labor_cost_minor\",\"overhead_cost_minor\",\"reported_output_milli\",\"reported_scrap_milli\",\"created_by\",\"version\",\"created_at\",\"updated_at\" FROM manufacturing_orders) manufacturing_orders"
} as const;}
