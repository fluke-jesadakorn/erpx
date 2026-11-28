import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {INVENTORY_RD_SERVICE_INITIAL_COUNT_ROW,INVENTORY_RD_SERVICE_COUNT_PERSISTED,nativeInventoryReadFirst} from './native-inventory-read';

/** Owner-native service physical-count source probe. The read is planned by
 * Rust and executed on the caller's already-open unit; no SQL lives here. */
export async function serviceInitialCountExists(database:SqlDatabase,countId:string,companyId:string):Promise<boolean>{
  return !!(await nativeInventoryReadFirst<Record<string,unknown>>(INVENTORY_RD_SERVICE_INITIAL_COUNT_ROW,[countId,companyId],database));
}

/** Marker detection deliberately ignores Company so corrupt or foreign markers
 * cannot be mistaken for ordinary legacy-owned counts. The read is planned by
 * Rust and runs on the caller's unit; no SQL lives in this bridge. */
export async function nativeServiceCountPersistedMarker(database:SqlDatabase,id:string):Promise<boolean>{
  const found=await nativeInventoryReadFirst<{marked:boolean}>(INVENTORY_RD_SERVICE_COUNT_PERSISTED,[id,id],database);
  return found?.marked===true;
}
