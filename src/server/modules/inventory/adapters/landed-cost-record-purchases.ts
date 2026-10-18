import "server-only";
import {hasPermission} from "@/lib/identity_access/permissions";
import type {WorkContext} from "../../identity_access/public/authorization";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {commercePackageEnforcementMode,readPurchasedAccess} from "../../commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "../../commercial/public/purchased-capability-sql";
import {landedCostPurchaseSourceRows,requireLandedCostPurchasedRead} from "./landed-cost-purchases";
/** Actual source and purchase are checked before shared consumer limits. */
export async function landedCostSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"staff"|"inbox"|"records"){
 const mode=await commercePackageEnforcementMode();if(mode!=="enforced"){if(mode==="shadow")await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"supply.advanced-inventory"});return {where:"TRUE",args:[] as unknown[]};}
 const id=surface==="staff"?"staff_notifications.record_id":surface==="inbox"?"e.source_id":"workspace_records.id",company=surface==="staff"?"staff_notifications.company_id":surface==="inbox"?"e.company_id":"workspace_records.company_id",relevant=surface==="records"?`EXISTS(SELECT 1 FROM inventory_landed_costs actual_landed_cost WHERE actual_landed_cost.id=${id})`:`${surface==="staff"?"staff_notifications.record_type":"e.source_type"}='landed_cost'`;
 if(actor.departmentId||!hasPermission(actor,"finance.read"))return {where:`NOT (${relevant})`,args:[]};
 const visible=`EXISTS(WITH landed_source_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT 1 FROM (SELECT s.*,NULL::text department_id FROM (${landedCostPurchaseSourceRows}) s) landed_source CROSS JOIN landed_source_clock WHERE landed_source.id=${id} AND landed_source.company_id=${company} AND (?::text IS NULL OR landed_source.branch_id=?) AND ${assignmentReadSql("landed_source","finance.read")} AND ${["supply.advanced-inventory","supply.receiving","supply.stock"].map(capability=>purchasedCapabilityPredicate("landed_source.company_id",capability,"history","landed_source_clock.now_ms")).join(" AND ")})`;
 return {where:`(NOT (${relevant}) OR ${visible})`,args:[actor.branchId,actor.branchId,actor.assignmentId,actor.userId,actor.role]};
}
export async function requireLandedCostWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,_moduleId:string,id:string){if(await commercePackageEnforcementMode()==="disabled")return;if(await database.prepare("SELECT 1 FROM inventory_landed_costs WHERE id=?").bind(id).first())await requireLandedCostPurchasedRead(database,actor,id);}
