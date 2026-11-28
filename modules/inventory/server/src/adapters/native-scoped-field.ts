import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {INVENTORY_RD_LOT_BALANCE_VERSION,INVENTORY_RD_TRANSFER_STATUS,nativeInventoryReadFirst} from './native-inventory-read';

/** Owner-native successors of the r33 scoped-field point reads. The Rust
 * registry owns both byte-identical statements; these bridges emit no SQL and
 * execute on the caller's already-open unit. */
export async function lotBalanceVersion(database:SqlDatabase,lotId:string|number|null,warehouseId:string|number|null):Promise<{version:number}|null>{
  return nativeInventoryReadFirst<{version:number}>(INVENTORY_RD_LOT_BALANCE_VERSION,[lotId,warehouseId],database);
}

export async function transferStatus(database:SqlDatabase,id:string,companyId:string):Promise<{status:string}|null>{
  return nativeInventoryReadFirst<{status:string}>(INVENTORY_RD_TRANSFER_STATUS,[id,companyId],database);
}
