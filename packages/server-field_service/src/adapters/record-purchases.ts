import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";

// Partition identity only. The retained generic Records engine cannot evaluate
// Field authorization or execute Field mutations. Actual Field Records source
// reads and guards belong to the native same-transaction composition.
const nativeIds="SELECT id FROM field_service_agreements UNION SELECT id FROM field_service_orders";
export async function serviceSourceReadScope(_database:SqlDatabase,_actor:WorkContext,_surface:"records"){
 void _database;void _actor;void _surface;
 return {where:`(workspace_records.module_id<>'field_service' AND NOT EXISTS(SELECT 1 FROM (${nativeIds}) native_field_record WHERE native_field_record.id=workspace_records.id))`,args:[] as (string|number|null)[]};
}
/** Fail closed for old generic/aliased workspace documents. An HTTP Field read
 * must never substitute for a native transactional Records guard. Remove this
 * partition adapter with the remaining generic Records engine. */
export async function requireServiceWorkspaceRecordRead(database:SqlDatabase,_actor:WorkContext,moduleId:string,id:string){
 void _actor;
 if(moduleId==="field_service"||await database.prepare(`SELECT 1 FROM (${nativeIds}) native_field_record WHERE id=?`).bind(id).first())
  throw new ApiError(503,"FIELD_RECORD_SOURCE_NATIVE_UNIT_REQUIRED","This Field document requires the native Records transaction.");
}
/** Native source-record consumers use the same current source in their
 * versioned guard, including purchase changes after the initial read. */
export async function serviceMetricHistoryAccess(_database:SqlDatabase,actor:WorkContext){
 void _database;const allowed=await (await import('./native-executive')).nativeFieldReportAccess(actor);
 return {"open-service-orders":allowed,"service-billing-releases":allowed,"field-visit-completion":allowed};
}
