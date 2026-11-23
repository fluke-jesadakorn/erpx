import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {ProjectionSource} from '@erpx/server-workflow/public/job-projector-contract';
import {nativeOperationsReadFirst,OPERATIONS_RD_TASK_PROJECTION_SOURCE} from './native-operations-read';
/** Owner-native read: the statement is planned by the Operations native read
 * port (`RD_TASK_PROJECTION_SOURCE`) and executed on the caller's open unit.
 * This module carries no SQL for this read. */
export function taskProjectionSource(db:SqlDatabase,id:string):Promise<ProjectionSource|null>{
 return nativeOperationsReadFirst<ProjectionSource>(OPERATIONS_RD_TASK_PROJECTION_SOURCE,[id],db);
}
