import "server-only";
import {
  COMMERCE_FULFILLMENT_LINES_SQL,
  COMMERCE_FULFILLMENTS_SQL,
  COMMERCE_ORDERS_SQL,
  COMMERCE_PRODUCTS_SQL,
  COMMERCE_SETTINGS_SQL,
  COMMERCE_WAREHOUSES_SQL,
} from "./projection-source-dedupe-sql";
/** Closed owner projections for Platform, Organization and Procurement joins. Original filters, bindings and supplied transactions remain with the caller. */
export const platformOwnerProjectionSources = Object.freeze({
  commerce_fulfillment_lines: COMMERCE_FULFILLMENT_LINES_SQL,
  commerce_fulfillments: COMMERCE_FULFILLMENTS_SQL,
  commerce_order_lines: "SELECT \"id\",\"order_id\",\"product_id\",\"description\",\"quantity_milli\",\"fulfilled_milli\",\"price_minor\",\"tax_bps\",\"subtotal_minor\",\"tax_minor\",\"ordinal\",\"list_price_minor\",\"discount_bps\" FROM commerce_order_lines",
  commerce_orders: COMMERCE_ORDERS_SQL,
  commerce_products: COMMERCE_PRODUCTS_SQL,
  commerce_settings: COMMERCE_SETTINGS_SQL,
  commerce_warehouses: COMMERCE_WAREHOUSES_SQL,
} as const);
