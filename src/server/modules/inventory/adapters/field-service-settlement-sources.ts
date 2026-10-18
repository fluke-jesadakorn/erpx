import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
/** Retain invalid/missing lineage as raw owner facts; a valid-only financial
 * projection must never erase an unsettled historical consumption. */
export const fieldServiceSettlementConsumptions="SELECT * FROM operational_inventory_consumptions WHERE source_kind='field_service'";
export const fieldServiceSettlementLotAllocations="SELECT * FROM operational_inventory_consumption_lots";
export const fieldServiceSettlementStock={products:"SELECT id,company_id,tracking_mode FROM commerce_products",warehouses:"SELECT id,company_id,branch_id FROM commerce_warehouses",lots:"SELECT id,company_id,product_id,tracking_mode FROM inventory_lots",moves:"SELECT * FROM inventory_lot_moves"} as const;
export function fieldServiceSettlementInventoryReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE commerce_products,commerce_warehouses,commerce_stock,inventory_lots,inventory_lot_balances,inventory_lot_moves,operational_inventory_consumptions,operational_inventory_consumption_lots IN SHARE MODE")];}
