import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {ScheduleRow} from "@erpx/server-operations/public/routines";
import {nativeOperationsReadAll,OPERATIONS_RD_ROUTINE_TEMPLATE_SCHEDULES} from './native-operations-read';
/** Owner-native read: the statement is planned by the Operations native read
 * port (`RD_ROUTINE_TEMPLATE_SCHEDULES`) and executed on the caller's open unit.
 * This module carries no SQL for this read. */
export function workUiStaffFacts(db:SqlDatabase){return {
async templateSchedules(value:string,companyId:string){
  return nativeOperationsReadAll<ScheduleRow>(OPERATIONS_RD_ROUTINE_TEMPLATE_SCHEDULES,[value,companyId],db);
}
};}
