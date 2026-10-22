import "server-only";
/** Closed native projections for Inventory, Planning, Manufacturing and Project snapshots. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const inventoryExecutiveOwnerProjections = Object.freeze({
  manufacturing_specifications: "SELECT \"id\",\"company_id\",\"name\",\"mode\",\"output_product_id\",\"batch_quantity_milli\",\"expected_yield_bps\",\"status\",\"created_by\",\"reviewed_by\",\"version\",\"created_at\",\"updated_at\" FROM manufacturing_specifications",
} as const);
