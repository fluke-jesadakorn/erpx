import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeInventoryReadFirst,INVENTORY_RD_POS_STARTUP_INVENTORY_STATE} from './native-inventory-read';
/** The complete Commerce chain already holds Stock/lot/reservation leases.
 * Its initial-setup proof also reads the native ledger: lease it explicitly
 * after those original writers' Stock leases and before the first snapshot. */
export function posStartupInventorySourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE inventory_counts,inventory_ledger,inventory_lot_balances IN SHARE MODE')];}
/** A posted zero-variance Count still owns its original posting decision. An
 * absent Company mapping cannot silently adopt that retained economic source.
 * Unposted Count intent is kept byte-exact without being called prior Stock. */
export async function posStartupInventoryState(database:SqlDatabase,companyId:string){
 const row=await nativeInventoryReadFirst<{economics:boolean;image:string}>(INVENTORY_RD_POS_STARTUP_INVENTORY_STATE,[companyId,companyId],database);
 if(!row)throw Error('Original Inventory posting state unavailable');return row;
}
