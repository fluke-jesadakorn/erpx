import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {INVENTORY_RD_COMMAND_REPLAY,INVENTORY_RD_PURCHASE_REPLAY,nativeInventoryReadFirst} from './native-inventory-read';

/** Owner-native command-receipt replay probes. The Rust registry owns both
 * byte-identical statements; these bridges emit no SQL and fail closed. */
export async function purchaseReplayExists(database:SqlDatabase,key:string,operation:string,id:string,userId:string,companyId:string):Promise<boolean>{
  return !!(await nativeInventoryReadFirst<Record<string,unknown>>(INVENTORY_RD_PURCHASE_REPLAY,[key,operation,id,userId,companyId],database));
}

export async function commandReplayExists(database:SqlDatabase,key:string,operation:string,userId:string,companyId:string):Promise<boolean>{
  return !!(await nativeInventoryReadFirst<Record<string,unknown>>(INVENTORY_RD_COMMAND_REPLAY,[key,operation,userId,companyId],database));
}
