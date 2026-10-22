import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {commerceQualitySourceReadLocks} from "@erpx/server-commerce/public/quality-purchase-sources";

/** Acquired before Service's first stock/lot snapshot. Writer leases precede
 * updates, rather than upgrading shared leases after another Company writes.
 * Quality origin relations follow the existing Quality/Count lock order. */
export function fieldServiceInventorySourceReadLocks(database:SqlDatabase) {
  return [...commerceQualitySourceReadLocks(database),
    database.prepare("LOCK TABLE inventory_quality_holds,inventory_quality_disposition_events,inventory_counts,inventory_count_lines,inventory_count_lot_lines IN SHARE ROW EXCLUSIVE MODE"),
    database.prepare("LOCK TABLE commerce_warehouses,commerce_products,commerce_settings IN SHARE MODE"),
    database.prepare("LOCK TABLE commerce_stock,inventory_lots,inventory_lot_balances,operational_inventory_consumptions,operational_inventory_consumption_lots IN SHARE ROW EXCLUSIVE MODE")];
}
