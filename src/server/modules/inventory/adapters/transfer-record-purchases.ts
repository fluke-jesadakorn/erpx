import "server-only";
import {hasPermission} from "@/lib/identity_access/permissions";
import type {WorkContext} from "../../identity_access/public/authorization";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {commercePackageEnforcementMode,readPurchasedAccess} from "../../commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "../../commercial/public/purchased-capability-sql";
import {requireTransferPurchasedRead} from "./transfer-purchases";

/** Filter actual warehouse-transfer identities before the consumer limit. Other
 * Inventory records, shared Stock facts and durable deliveries retain owners. */
export async function transferSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"staff"|"inbox"|"records"){
 const mode=await commercePackageEnforcementMode();
 if(mode!=="enforced"){
  if(mode==="shadow")await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"supply.transfers"});
  return {where:"TRUE",args:[] as unknown[]};
 }
 const id=surface==="staff"?"staff_notifications.record_id":surface==="inbox"?"e.source_id":"workspace_records.id";
 const company=surface==="staff"?"staff_notifications.company_id":surface==="inbox"?"e.company_id":"workspace_records.company_id";
 const relevant=surface==="records"?`(module_id='inventory' AND (EXISTS(SELECT 1 FROM inventory_transfers native_transfer WHERE native_transfer.id=${id}) OR EXISTS(SELECT 1 FROM inventory_transit native_transit WHERE native_transit.id=${id})))`:`${surface==="staff"?"staff_notifications.record_type":"e.source_type"} IN ('transfer','stock_transfer','inventory_transfer','inventory_transit')`;
 if(actor.departmentId||!hasPermission(actor,"inventory.read"))return {where:`NOT (${relevant})`,args:[]};
 const parent=`FROM inventory_transit d JOIN inventory_transfers t ON t.id=d.transfer_id AND t.company_id=d.company_id AND t.source_warehouse_id=d.source_warehouse_id AND t.target_warehouse_id=d.target_warehouse_id AND t.source_branch_id=d.source_branch_id AND t.target_branch_id=d.target_branch_id AND t.delivery_mode='transit'`;
 const sources=`SELECT id,company_id,source_branch_id branch_id,NULL::text department_id FROM inventory_transfers UNION ALL SELECT id,company_id,target_branch_id,NULL::text FROM inventory_transfers UNION ALL SELECT d.id,d.company_id,d.source_branch_id,NULL::text ${parent} UNION ALL SELECT d.id,d.company_id,d.target_branch_id,NULL::text ${parent}`;
 const visible=`EXISTS(WITH transfer_source_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT 1 FROM (${sources}) transfer_source CROSS JOIN transfer_source_clock WHERE transfer_source.id=${id} AND transfer_source.company_id=${company} AND (?::text IS NULL OR transfer_source.branch_id=?) AND ${assignmentReadSql("transfer_source","inventory.read")} AND ${purchasedCapabilityPredicate("transfer_source.company_id","supply.transfers","history","transfer_source_clock.now_ms")})`;
 return {where:`(NOT (${relevant}) OR ${visible})`,args:[actor.branchId,actor.branchId,actor.assignmentId,actor.userId,actor.role]};
}
export async function requireTransferWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 if(moduleId!=="inventory"||await commercePackageEnforcementMode()==="disabled")return;
 if(await database.prepare("SELECT 1 FROM inventory_transfers WHERE id=?").bind(id).first())await requireTransferPurchasedRead(database,actor,{kind:"transfer",id});
 else if(await database.prepare("SELECT 1 FROM inventory_transit WHERE id=?").bind(id).first())await requireTransferPurchasedRead(database,actor,{kind:"transit",id});
}
