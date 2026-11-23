import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {nativeOperationsReadFirst,OPERATIONS_RD_APP_UPLOAD_TASK_STATE} from './native-operations-read';
/** Owner-native read: the statement is planned by the Operations native read
 * port (`RD_APP_UPLOAD_TASK_STATE`) and executed on the caller's open unit.
 * This module carries no SQL for this read. */
export function appUploadTaskState(db: SqlDatabase, id: string, companyId: string): Promise<{ state: string } | null>{
 return nativeOperationsReadFirst<{ state: string }>(OPERATIONS_RD_APP_UPLOAD_TASK_STATE,[id,companyId],db);
}
