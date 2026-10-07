import "server-only";
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  commerce_stock: "SELECT \"warehouse_id\",\"product_id\",\"quantity_milli\",\"value_minor\",\"version\" FROM commerce_stock",
  inventory_counts: "SELECT \"id\",\"company_id\",\"branch_id\",\"warehouse_id\",\"number\",\"count_date\",\"reason\",\"status\",\"version\",\"net_value_minor\",\"created_by\",\"prepared_by\",\"approved_by\",\"review_reason\",\"submitted_at\",\"posted_at\",\"journal_id\",\"created_at\",\"updated_at\",\"quality_hold_id\" FROM inventory_counts",
  inventory_ledger: "SELECT \"id\",\"warehouse_id\",\"product_id\",\"quantity_milli\",\"value_minor\",\"created_at\",\"movement_date\",\"source_type\",\"source_id\",\"source_number\" FROM inventory_ledger",
  inventory_lot_balances: "SELECT \"lot_id\",\"warehouse_id\",\"quantity_milli\",\"value_minor\",\"version\" FROM inventory_lot_balances",
  inventory_lots: "SELECT \"id\",\"company_id\",\"product_id\",\"tracking_mode\",\"lot_code\",\"serial_number\",\"manufactured_date\",\"expiry_date\",\"status\",\"created_by\",\"created_at\",\"version\" FROM inventory_lots",
  inventory_order_reservations: "SELECT \"id\",\"order_line_id\",\"warehouse_id\",\"product_id\",\"quantity_milli\",\"consumed_milli\",\"released_milli\",\"expires_at\",\"created_by\",\"reason\",\"release_reason\",\"created_at\" FROM inventory_order_reservations",
} as const;
