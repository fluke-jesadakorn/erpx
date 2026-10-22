import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Raw immutable maintenance issues remain visible even if a foreign or
 * missing Work/financial relationship makes the owner source ambiguous. */
export const maintenanceSettlementConsumptions="SELECT * FROM operational_inventory_consumptions WHERE source_kind='maintenance'";
export const maintenanceSettlementLotAllocations="SELECT * FROM operational_inventory_consumption_lots";
export const maintenanceSettlementStockSources={products:"SELECT id,company_id,tracking_mode FROM commerce_products",warehouses:"SELECT id,company_id,branch_id FROM commerce_warehouses",lots:"SELECT id,company_id,product_id,tracking_mode FROM inventory_lots",moves:"SELECT * FROM inventory_lot_moves"} as const;
export function maintenanceSettlementInventoryReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE operational_inventory_consumptions,operational_inventory_consumption_lots,inventory_lots,inventory_lot_moves,commerce_products,commerce_warehouses IN SHARE MODE")];}
