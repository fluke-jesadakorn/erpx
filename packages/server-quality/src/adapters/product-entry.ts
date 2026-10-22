import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {qualitySourceAccess} from "@erpx/server-quality/adapters/purchases";
import {countSourceAccess} from "@erpx/server-inventory/public/count-purchases";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {QualitySourceEntry,QualityProductEntryScope,QualityProductPresentationOptions} from "@erpx/server-quality/domain/product-entry";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {readPurchasedAccesses} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";
import {salesQualityCountProductEntry} from "@erpx/server-inventory/public/quality-product-entry";
import {qualityPurchaseSourceRows} from "@erpx/server-quality/adapters/purchase-sources";
import {qualitySourcePurchasePredicate} from "@erpx/server-quality/adapters/record-purchases";
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {workContextGuards,financialConstraintError} from "@erpx/server-workflow/public/commands";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {ApiError} from "@erpx/server-runtime/public/http";
/** Read-only role candidate discovery. Entry never creates a WorkContext,
 * chooses an assignment or returns a private source/purchase snapshot. */
export async function qualityProductSourceEntry(database:SqlDatabase,scope:QualityProductEntryScope,target:QualitySourceEntry){
 if(scope.departmentId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 if(target.kind==="count")return salesQualityCountProductEntry(database,scope,target.id);
 const row=await database.prepare(`WITH quality_entry_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms)
 SELECT quality_entry.kind FROM (SELECT s.*,NULL::text department_id FROM (${qualityPurchaseSourceRows}) s) quality_entry CROSS JOIN quality_entry_clock
 WHERE quality_entry.id=? AND quality_entry.company_id=? AND (?::text IS NULL OR quality_entry.branch_id=?)
 AND ${assignmentReadSql("quality_entry","quality.read")} AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND version=?)
 AND ${scheduledFeaturePredicate("quality_entry.company_id","quality")}
 AND ${qualitySourcePurchasePredicate("quality_entry","history","quality_entry_clock.now_ms")}`)
 .bind(target.id,scope.companyId,scope.branchId,scope.branchId,scope.assignmentId,scope.userId,scope.role,scope.assignmentId,scope.assignmentVersion).first<{kind:string}>();
 return row?{capability:row.kind==="customer-return"?"sales.orders":row.kind==="receipt"?"supply.receiving":"supply.quality",purchasePurpose:"history" as const}:null;
}

export type QualitySourceBackdrop={kind:QualitySourceEntry["kind"];title:string;description:string;status:string};
async function strictSourceScope(database:SqlDatabase,actor:WorkContext,target:QualitySourceEntry,captured?:QualityProductEntryScope){
 const version=captured?.assignmentVersion??actor.authority?.assignmentVersion??actor.iam?.queuedAssignmentVersion??await database.prepare("SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=?").bind(actor.assignmentId,actor.userId,actor.companyId,actor.role).first<number>("version");
 const scope=captured??{companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:version??0,role:actor.role},permission=target.kind==="hold"?"quality.read":"inventory.read";
 const current=await database.prepare(`SELECT ${assignmentReadSql("quality_product_scope",permission)} allowed
  FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) quality_product_scope
  WHERE EXISTS(SELECT 1 FROM assignments WHERE id=? AND version=?)
  AND ${scheduledFeaturePredicate("quality_product_scope.company_id",target.kind==="hold"?"quality":"inventory")}`)
  .bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId,actor.assignmentId,scope.assignmentVersion).first<{allowed:boolean}>();
 if(!current?.allowed||actor.departmentId)throw new ApiError(403,"QUALITY_SOURCE_ACCESS_CHANGED","Choose a current authorized source assignment.");
 const key=`quality-product-presentation:${crypto.randomUUID()}`;try{await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);}catch(error){throw financialConstraintError(error)??error;}
 return scope;
}
/** The canonical detail drawer repeats its original owner authorization. Its
 * background must not load an unavailable management collection. */
export async function qualitySourceRecordBackdrop(actor:WorkContext,target:QualitySourceEntry,options:QualityProductPresentationOptions={}):Promise<QualitySourceBackdrop|null>{
 const database=(await holdingResources(actor.holdingId)).write;
 let strictScope:QualityProductEntryScope|undefined;
 if(options.strictProduct){
  const scope=await strictSourceScope(database,actor,target);
  const capability=target.kind==="hold"?"supply.quality":"supply.counts",collection=(await readPurchasedAccesses(database,"enforced",actor.companyId,[capability]))[capability];if(collection.history)return null;
  if(!await qualityProductSourceEntry(database,scope,target))throw new ApiError(404,"QUALITY_SOURCE_NOT_FOUND","The original purchased Quality source is unavailable.");strictScope=scope;
 }else{const collection=target.kind==="hold"?await qualitySourceAccess(database,actor):await countSourceAccess(database,actor);if(!collection||collection.history)return null;}
 // These are original authenticated owner readers; no candidate assignment
 // is converted to a new acting context and only the summary crosses UI.
 if(target.kind==="hold"){
  const {qualityDetail}=await import("@erpx/server-quality/public/quality"),{hold}=await qualityDetail(actor,target.id);
  if(strictScope){await strictSourceScope(database,actor,target,strictScope);if(!await qualityProductSourceEntry(database,strictScope,target))throw new ApiError(404,"QUALITY_SOURCE_NOT_FOUND","The original purchased Quality source is unavailable.");}
  return {kind:"hold",title:`Quality hold · ${String(hold.product_name)}`,description:String(hold.warehouse_name),status:String(hold.status)};
 }
 const {countDetail}=await import("@erpx/server-inventory/public/inventory-counts"),{count}=await countDetail(actor,target.id);
 if(strictScope){await strictSourceScope(database,actor,target,strictScope);if(!await qualityProductSourceEntry(database,strictScope,target))throw new ApiError(404,"QUALITY_SOURCE_NOT_FOUND","The original purchased Quality source is unavailable.");}
 return {kind:"count",title:`Stock count · ${String(count.number)}`,description:String(count.warehouse_name),status:String(count.status)};
}
