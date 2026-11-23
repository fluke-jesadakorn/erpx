import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {nativeOperationsReadFirst,OPERATIONS_RD_APP_UPLOAD_TASK_STATE} from './native-operations-read';
/** Owner-native read: the identical task-state literal was already planned by
 * the Operations native read port (`RD_APP_UPLOAD_TASK_STATE`). This caller
 * reuses that plan instead of binding its own SQL; it carries no SQL for this
 * read and returns the scalar `state` the retired `.first('state')` produced. */
export function collaborationHomeSources(db: SqlDatabase) { return {
 async trackingState(task_id: string, companyId: string) {
  const row = await nativeOperationsReadFirst<{ state: string }>(OPERATIONS_RD_APP_UPLOAD_TASK_STATE, [task_id, companyId], db);
  return row?.state ?? null;
 }
}; }
