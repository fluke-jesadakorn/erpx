import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {INVENTORY_RD_SERVICE_INITIAL_COUNT_ROW,nativeInventoryReadFirst} from './native-inventory-read';

/** Owner-native service physical-count source probe. The read is planned by
 * Rust and executed on the caller's already-open unit; no SQL lives here. */
export async function serviceInitialCountExists(database:SqlDatabase,countId:string,companyId:string):Promise<boolean>{
  return !!(await nativeInventoryReadFirst<Record<string,unknown>>(INVENTORY_RD_SERVICE_INITIAL_COUNT_ROW,[countId,companyId],database));
}
