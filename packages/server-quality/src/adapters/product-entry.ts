import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {qualitySourceAccess} from "@erpx/server-quality/adapters/purchases";
import {countSourceAccess} from "@erpx/server-inventory/public/count-purchases";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {QualitySourceEntry,QualityProductEntryScope,QualityProductPresentationOptions} from "@erpx/server-quality/domain/product-entry";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {readPurchasedAccesses} from "@erpx/server-commercial/public/purchase-command-boundary";
import {qualityClockCte} from "@erpx/server-quality/adapters/quality-scope-sql";
import {salesQualityCountProductEntry} from "@erpx/server-inventory/public/quality-product-entry";
import {qualityPurchaseSourceRows} from "@erpx/server-quality/adapters/purchase-sources";
import {qualitySourcePurchasePredicate} from "@erpx/server-quality/adapters/record-purchases";
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {workContextGuards,financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {ApiError} from "@erpx/server-runtime/public/http";
import {clearWorkflowCommandAssertions} from "@erpx/server-workflow/public/command-assertion-effects";
import {nativeQualityHoldDetail} from '../../../feature-quality/src/public/server';
/** Read-only role candidate discovery. Entry never creates a WorkContext,
 * chooses an assignment or returns a private source/purchase snapshot. */
export async function qualityProductSourceEntry(database:SqlDatabase,scope:QualityProductEntryScope,target:QualitySourceEntry){
 if(scope.departmentId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 if(target.kind==="count")return salesQualityCountProductEntry(database,scope,target.id);
 const row=await database.prepare(`${qualityClockCte("quality_entry_clock")}
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
  // Current assignment, Company/branch scope, permission and purchase history
  // are verified inside the native detail_core owner unit by the caller's
  // `nativeQualityHoldDetail` read. No duplicate TypeScript policy SELECT is
  // retained here; the captured version is only a display-time input.
  const version=captured?.assignmentVersion??actor.authority?.assignmentVersion??actor.iam?.queuedAssignmentVersion??0;
  if(actor.departmentId||!Number.isSafeInteger(version)||version<1)throw new ApiError(403,"QUALITY_SOURCE_ACCESS_CHANGED","Choose a current authorized source assignment.");
  const scope=captured??{companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:version,role:actor.role};
  const key=`quality-product-presentation:${crypto.randomUUID()}`;try{await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),clearWorkflowCommandAssertions(database, key)]);}catch(error){throw financialConstraintError(error)??error;}
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
  const detail=await nativeQualityHoldDetail(actor,target.id),hold=detail.hold;
  return {kind:"hold",title:`Quality hold · ${hold.productName}`,description:hold.warehouseName,status:hold.status};
 }
 const {countDetail}=await import("@erpx/server-inventory/public/inventory-counts"),{count}=await countDetail(actor,target.id);
  if(strictScope){await strictSourceScope(database,actor,target,strictScope);if(!await qualityProductSourceEntry(database,strictScope,target))throw new ApiError(404,"QUALITY_SOURCE_NOT_FOUND","The original purchased Quality source is unavailable.");}
 return {kind:"count",title:`Stock count · ${String(count.number)}`,description:String(count.warehouse_name),status:String(count.status)};
}
