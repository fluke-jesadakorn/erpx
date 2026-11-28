import "server-only";
import {
  COMMERCE_FULFILLMENT_LINES_SQL,
  COMMERCE_FULFILLMENTS_SQL,
  COMMERCE_RETURNS_SQL,
  COMMERCE_SETTINGS_SQL,
  COMMERCE_WAREHOUSES_SQL,
} from "./projection-source-dedupe-sql";
/** Closed native projections for Inventory, Planning, Manufacturing and Project snapshots. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const inventoryExecutiveOwnerProjections = Object.freeze({
  commerce_fulfillment_lines: COMMERCE_FULFILLMENT_LINES_SQL,
  commerce_fulfillments: COMMERCE_FULFILLMENTS_SQL,
  commerce_order_lines: "SELECT \"id\",\"order_id\",\"product_id\",\"description\",\"quantity_milli\",\"fulfilled_milli\",\"price_minor\",\"tax_bps\",\"subtotal_minor\",\"tax_minor\",\"ordinal\",\"list_price_minor\",\"discount_bps\",\"item_type\" FROM commerce_order_lines",
  commerce_products: "SELECT \"id\",\"company_id\",\"sku\",\"name\",\"unit\",\"price_minor\",\"tax_bps\",\"status\",\"version\",\"tracking_mode\",\"item_type\" FROM commerce_products",
  commerce_return_lines: "SELECT \"id\",\"return_id\",\"fulfillment_line_id\",\"product_id\",\"quantity_milli\",\"value_minor\",\"subtotal_minor\",\"tax_minor\",\"ordinal\",\"condition_code\",\"condition_note\",\"quality_due_date\",\"expected_subtotal_minor\" FROM commerce_return_lines",
  commerce_return_lot_allocations: "SELECT \"id\",\"return_line_id\",\"fulfillment_allocation_id\",\"lot_id\",\"quantity_milli\",\"value_minor\",\"snapshot_quantity_milli\",\"snapshot_value_minor\",\"snapshot_version\" FROM commerce_return_lot_allocations",
  commerce_returns: COMMERCE_RETURNS_SQL,
  commerce_settings: COMMERCE_SETTINGS_SQL,
  commerce_warehouses: COMMERCE_WAREHOUSES_SQL,
} as const);
