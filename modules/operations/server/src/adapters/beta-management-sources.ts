import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeOperationsReadFirst,nativeOperationsReadSql,OPERATIONS_RD_BETA_MANAGEMENT_TASK_COMPANY,OPERATIONS_RD_BETA_MANAGEMENT_TASK_FILE_OWNER} from './native-operations-read';
/** Owner-native reads: both statements are planned by the Operations native read
 * port, so this module carries no SQL. `taskCompany` runs the planned read on the
 * caller's open unit; `taskFileOwner` returns the planned correlated fragment for
 * the trusted caller to embed in its own attachment query (its `?` is bound there). */
export function betaManagementTaskSources(db:SqlDatabase){return {
 async taskFileOwner(){return (await nativeOperationsReadSql(OPERATIONS_RD_BETA_MANAGEMENT_TASK_FILE_OWNER)).sql;},
 async taskCompany(id:string){return nativeOperationsReadFirst<{company_id:string}>(OPERATIONS_RD_BETA_MANAGEMENT_TASK_COMPANY,[id],db);},
};}
