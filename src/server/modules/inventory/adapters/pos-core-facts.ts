import "server-only";
import {one} from "../../pos/public/pos-core";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function posCoreInventoryFacts(db:SqlDatabase){return {
readInventoryActiveReservations1:(facts:{warehouse_id:string | number | boolean | null;productId:string;})=>one(db,'SELECT COALESCE(SUM(quantity_milli),0) n FROM inventory_active_reservations WHERE warehouse_id=? AND product_id=?',facts.warehouse_id,facts.productId),
readInventoryLedger1:(facts:{warehouse_id:string | number | boolean | null;productId:string;business_date:string | number | boolean | null;})=>one(db,'SELECT id FROM inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>? LIMIT 1',facts.warehouse_id,facts.productId,facts.business_date)
};}
