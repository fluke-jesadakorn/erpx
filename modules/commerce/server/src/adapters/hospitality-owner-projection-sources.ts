import "server-only";
import {
  COMMERCE_ORDERS_SQL,
  COMMERCE_PRODUCTS_SQL,
  COMMERCE_SETTINGS_SQL,
  COMMERCE_WAREHOUSES_SQL,
} from "./projection-source-dedupe-sql";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  commerce_fulfillment_evidence: "SELECT \"fulfillment_id\",\"attachment_id\",\"linked_by\",\"linked_at\" FROM commerce_fulfillment_evidence",
  commerce_orders: COMMERCE_ORDERS_SQL,
  commerce_products: COMMERCE_PRODUCTS_SQL,
  commerce_settings: COMMERCE_SETTINGS_SQL,
  commerce_warehouses: COMMERCE_WAREHOUSES_SQL,
} as const;

/** Query-free source lease for actual hospitality Stock ownership joins. */
export function hospitalitySettlementReferenceReadLocks(database:import("@erpx/platform-runtime-server/public/runtime/types").SqlDatabase){return [database.prepare("LOCK TABLE commerce_warehouses,commerce_products IN SHARE MODE")];}
