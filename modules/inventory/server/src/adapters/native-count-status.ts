import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {INVENTORY_RD_COUNT_STATUS,nativeInventoryReadFirst} from './native-inventory-read';

/** Owner-native canonical count-status point read. The Rust registry owns the
 * byte-identical statement; this bridge emits no SQL and fails closed. */
export async function countStatus(database:SqlDatabase,id:string,companyId:string):Promise<{status:string}|null>{
  return nativeInventoryReadFirst<{status:string}>(INVENTORY_RD_COUNT_STATUS,[id,companyId],database);
}
