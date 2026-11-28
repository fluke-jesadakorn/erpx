import "server-only";
import {
  COMMERCE_FULFILLMENTS_SQL,
  COMMERCE_ORDERS_SQL,
  COMMERCE_PRODUCTS_SQL,
  COMMERCE_RETURNS_SQL,
  COMMERCE_SETTINGS_SQL,
  COMMERCE_WAREHOUSES_SQL,
} from "./projection-source-dedupe-sql";
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  commerce_fulfillments: COMMERCE_FULFILLMENTS_SQL,
  commerce_orders: COMMERCE_ORDERS_SQL,
  commerce_products: COMMERCE_PRODUCTS_SQL,
  commerce_returns: COMMERCE_RETURNS_SQL,
  commerce_settings: COMMERCE_SETTINGS_SQL,
  commerce_warehouses: COMMERCE_WAREHOUSES_SQL,
} as const;
