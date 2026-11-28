import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {
  nativeInventoryReadFirst,
  INVENTORY_RD_ACTIVE_RESERVATIONS_TOTAL,
  INVENTORY_RD_LEDGER_MOVEMENT_AFTER,
} from './native-inventory-read';
/** Owner-native reads: planned by the Inventory read registry and executed on
 * the caller's unit; this module carries no SQL for them. */
export function posCoreInventoryFacts(db:SqlDatabase){return {
readInventoryActiveReservations1:(facts:{warehouse_id:string | number | boolean | null;productId:string;})=>nativeInventoryReadFirst<Record<string,string|number|boolean|null>>(INVENTORY_RD_ACTIVE_RESERVATIONS_TOTAL,[facts.warehouse_id,facts.productId],db),
readInventoryLedger1:(facts:{warehouse_id:string | number | boolean | null;productId:string;business_date:string | number | boolean | null;})=>nativeInventoryReadFirst<Record<string,string|number|boolean|null>>(INVENTORY_RD_LEDGER_MOVEMENT_AFTER,[facts.warehouse_id,facts.productId,facts.business_date],db)
};}
