import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {WorkContext} from "../../identity_access/public/authorization";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import {hasPermission} from "@/lib/identity_access/permissions";
import {commercePackageEnforcementMode,readPurchasedAccess,checkPurchasedCapability} from "../../commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "../../commercial/public/purchased-capability-sql";
import {ApiError} from "../../runtime/public/http";
import {supplierBillOwnerPredicate} from "./bill-source-sql";

export async function procurementSourcePredicate(database:SqlDatabase,actor:WorkContext|undefined,alias:string,purchased=true){
 return sourcePredicate(database,actor,alias,"commerce.read",purchased);
}
export async function supplierBillSourcePredicate(database:SqlDatabase,actor:WorkContext|undefined,alias:string,purchased=true){
 const scope=await sourcePredicate(database,actor,alias,"finance.read",purchased);
 if(await commercePackageEnforcementMode()==="disabled")return scope;
 return {...scope,where:`(${scope.where}) AND ${supplierBillOwnerPredicate(alias)}`};
}
async function sourcePredicate(database:SqlDatabase,actor:WorkContext|undefined,alias:string,permission:"commerce.read"|"finance.read",purchased:boolean){
 const mode=await commercePackageEnforcementMode();
 if(mode==="disabled")return {where:"TRUE",args:[] as unknown[]};
 if(!actor||!hasPermission(actor,permission))return {where:"FALSE",args:[] as unknown[]};
 if(mode==="shadow")await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"supply.purchasing"});
 const purchase=mode==="enforced"&&purchased?`EXISTS(WITH procurement_source_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT 1 FROM procurement_source_clock WHERE ${purchasedCapabilityPredicate(`${alias}.company_id`,"supply.purchasing","history","procurement_source_clock.now_ms")})`:"TRUE";
 return {where:`${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?) AND ${assignmentReadSql(alias,permission)} AND ${purchase}`,args:[actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,actor.assignmentId,actor.userId,actor.role]};
}
/** These predicates resolve actual RFQs and actual purchase orders before each
 * shared surface's LIMIT. A notification label cannot turn a Sales order into
 * a Supply source, or open another Company through an old projection. */
export async function procurementSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"staff"|"inbox"|"records"){
 const mode=await commercePackageEnforcementMode();if(mode!=="enforced"){
  if(mode==="shadow")await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"supply.purchasing"});
  return {where:"TRUE",args:[] as unknown[]};
 }
 const id=surface==="records"?"workspace_records.id":surface==="staff"?"staff_notifications.record_id":"e.source_id",company=surface==="records"?"workspace_records.company_id":surface==="staff"?"staff_notifications.company_id":"e.company_id";
 const rfq=await procurementSourcePredicate(database,actor,"procurement_source"),visible=`EXISTS(SELECT 1 FROM procurement_rfqs procurement_source WHERE procurement_source.id=${id} AND procurement_source.company_id=${company} AND ${rfq.where})`;
 const bill=await supplierBillSourcePredicate(database,actor,"procurement_bill_source"),billVisible=`EXISTS(SELECT 1 FROM procurement_bill_matches procurement_bill_source WHERE procurement_bill_source.id=${id} AND procurement_bill_source.company_id=${company} AND ${bill.where})`;
 if(surface==="records")return {where:`(NOT(module_id='procurement' AND (EXISTS(SELECT 1 FROM procurement_rfqs actual_rfq WHERE actual_rfq.id=${id}) OR EXISTS(SELECT 1 FROM procurement_bill_matches actual_bill WHERE actual_bill.id=${id}))) OR ${visible} OR ${billVisible})`,args:[...rfq.args,...bill.args]};
 const type=surface==="staff"?"staff_notifications.record_type":"e.source_type",promise=await procurementSourcePredicate(database,actor,"procurement_promise_source");
 return {where:`(${type} NOT IN ('procurement_rfq','supplier_promise','supplier_bill') OR (${type}='procurement_rfq' AND ${visible}) OR (${type}='supplier_promise' AND EXISTS(SELECT 1 FROM commerce_orders procurement_promise_source WHERE procurement_promise_source.id=${id} AND procurement_promise_source.company_id=${company} AND procurement_promise_source.kind='purchase' AND ${promise.where})) OR (${type}='supplier_bill' AND ${billVisible}))`,args:[...rfq.args,...promise.args,...bill.args]};
}
export async function requireProcurementWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 const mode=await commercePackageEnforcementMode();if(moduleId!=="procurement"||mode==="disabled")return;
 const bill=!!await database.prepare("SELECT 1 FROM procurement_bill_matches WHERE id=?").bind(id).first();
 if(!bill&&!await database.prepare("SELECT 1 FROM procurement_rfqs WHERE id=?").bind(id).first())return;
 const scope=await (bill?supplierBillSourcePredicate:procurementSourcePredicate)(database,actor,"r",false);
 if(!await database.prepare(`SELECT 1 FROM ${bill?"procurement_bill_matches":"procurement_rfqs"} r WHERE r.id=? AND ${scope.where}`).bind(id,...scope.args).first())throw new ApiError(404,bill?"SUPPLIER_BILL_NOT_FOUND":"RFQ_NOT_FOUND","The actual Procurement source was not found in this workspace.");
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:"supply.purchasing",purpose:"history"});
}
