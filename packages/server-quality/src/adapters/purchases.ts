import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {workContextGuards,financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {commercePackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {flushFinanceDeferredEffects} from "@erpx/server-finance/public/finance-transactions";
import {isTransactionalSqlDatabase,type SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {qualityHoldPurchaseSource,qualityPurchaseSourceReadLocks,type QualityPurchaseSource} from "@erpx/server-quality/adapters/purchase-sources";
import {qualityWorkScopeTuple} from "@erpx/server-quality/adapters/quality-work-scope-sql";
import {qualitySourceArmHead} from "./quality-source-arm-head";
import {evidenceHead} from "./inspection-evidence-source-sql";
import {nativeQualityReadFirst,RD_ALERT_HOLD_ID,RD_RECEIPT_REPLAY} from "./native-quality-read";
import {clearWorkflowCommandAssertions} from "@erpx/server-workflow/public/command-assertion-effects";
type Actor=Pick<WorkContext,"userId"|"assignmentId"|"role"|"companyId"|"branchId"|"departmentId"|"authority"|"iam">;
export type QualityPurchaseTarget={kind:"hold"|"spec"|"alert";id:string};
export type QualityPurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
const capabilities=["supply.quality","supply.receiving","supply.stock","sales.orders","inventory.stock-core"] as const;
/** Canonical alert-to-hold id lookup shared by the Quality purchase-scope readers.
 * The statement is planned by the native Quality owner read `RD_ALERT_HOLD_ID`
 * and executed on the caller's already-open unit; no TypeScript SQL mirror
 * remains and the unscoped purchase-gate semantics are unchanged. */
export async function qualityAlertHoldId(database:SqlDatabase,id:string):Promise<string|undefined>{
 return (await nativeQualityReadFirst<{hold_id:string}>(RD_ALERT_HOLD_ID,[id],database))?.hold_id;
}
async function current(database:SqlDatabase,actor:Actor,key:string,permissions:readonly string[]=[]){
 if(actor.departmentId)throw new ApiError(403,"QUALITY_ACCESS_CHANGED","Choose a current Company or Branch Quality assignment.");
 for(const permission of new Set(["quality.read",...permissions])){
  if(!hasPermission(actor,permission))throw new ApiError(403,"QUALITY_ACCESS_CHANGED","Choose a current authorized Quality assignment.");
  const row=await database.prepare(evidenceHead(`${assignmentReadSql("quality_scope",permission)} allowed`,` FROM ${qualityWorkScopeTuple("?::text company_id,?::text branch_id,NULL::text department_id","quality_scope")}`,'')).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId).first<{allowed:boolean}>();
  if(!row?.allowed)throw new ApiError(403,"QUALITY_ACCESS_CHANGED","The original Quality assignment is no longer current.");
 }
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),clearWorkflowCommandAssertions(database, key)]);
}
async function targetSource(database:SqlDatabase,actor:Actor,target:QualityPurchaseTarget):Promise<{capabilities:readonly string[];witness:string;owner?:QualityPurchaseSource;productId:string}>{
 if(target.kind==="spec"){
  const row=await database.prepare(qualitySourceArmHead("id,company_id,product_id,created_by","inventory_quality_specs"," WHERE id=? AND company_id=?")).bind(target.id,actor.companyId).first<{id:string;company_id:string;product_id:string;created_by:string}>();
  if(!row)throw new ApiError(404,"QUALITY_SPEC_NOT_FOUND","The original inspection specification is unavailable.");return {capabilities:["supply.quality"],witness:JSON.stringify(row),productId:row.product_id};
 }
 const id=target.kind==="alert"?await qualityAlertHoldId(database,target.id):target.id;
 if(!id)throw new ApiError(404,"QUALITY_ALERT_NOT_FOUND","The original Quality escalation is unavailable.");
 const owner=await qualityHoldPurchaseSource(database,actor,id);return {capabilities:owner.capabilities,witness:owner.witness,owner,productId:owner.hold.product_id};
}
export async function qualitySourceAccess(database:SqlDatabase,actor:Actor,id?:string):Promise<QualityPurchaseControls|undefined>{
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return undefined;
 await current(database,actor,`quality-read:${randomUUID()}`);const source=id?await qualityHoldPurchaseSource(database,actor,id):null,access=await readPurchasedAccesses(database,mode,actor.companyId,capabilities),required=source?.capabilities??["supply.quality"];
 return {newWork:(!source||source.kind==="advanced")&&required.every(capability=>access[capability].newWork),settleExisting:required.every(capability=>access[capability].settleExisting),history:required.every(capability=>access[capability].history),revision:access["supply.quality"].revision};
}
export async function qualityPurchaseControls(actor:WorkContext,id?:string){return qualitySourceAccess((await holdingResources(actor.holdingId)).write,actor,id);}
export async function requireQualityPurchasedRead(database:SqlDatabase,actor:Actor,id?:string){
 const access=await qualitySourceAccess(database,actor,id);if(access&&!access.history)throw new ApiError(403,"PACKAGE_REQUIRED","This Quality source requires its original purchase or retained history.");return access;
}
export async function qualityVisibleHolds<Row extends Record<string,unknown>>(database:SqlDatabase,actor:Actor,rows:Row[]):Promise<Row[]>{
 if(await commercePackageEnforcementMode()==="disabled")return rows;
 const visible:Row[]=[];for(const row of rows){const access=await qualitySourceAccess(database,actor,String(row.id));if(access?.history)visible.push(row);}return visible;
}
/** Full Quality owner only. Protective receiving/return hold effects remain
 * composed under their original Commerce owner, never a global Stock hook. */
export async function withQualityPurchasedCommand<Result>(owner:SqlDatabase,actor:Actor,command:{key:string;operation:string;target:QualityPurchaseTarget;purpose:CapabilityPurpose;permissions:readonly string[];existing?:boolean;disposition?:boolean;expected?:{productId:string;warehouseId?:string;lotId?:string}},run:(database:SqlDatabase)=>Promise<Result>){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return run(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"QUALITY_TRANSACTION_REQUIRED","Quality requires its original owned Holding transaction.");
 try{return await owner.transaction(async database=>{
  const companyLocks=companyPurchaseReadLocks(database,actor.companyId);await database.batch([companyLocks[0],...qualityPurchaseSourceReadLocks(database),...companyLocks.slice(1)]);
  const replay=await nativeQualityReadFirst(RD_RECEIPT_REPLAY,[command.key,command.operation,command.target.id,actor.userId,actor.companyId],database);
  const before=command.existing||replay?await targetSource(database,actor,command.target):null,purpose=replay?"history" as const:command.purpose;
  const required=before?.capabilities??["supply.quality"],extra=command.disposition&&before?.owner?.kind!=="customer-return"?["supply.counts"]:[],scopes=[...new Set([...required,...extra])].map(capability=>({companyId:actor.companyId,capability,purpose}));
  const key=`quality-purchase:${randomUUID()}`,boundaries=scopes.map((scope,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,mode,`${key}:${index}`,scope)!}));
  for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);await current(database,actor,key,command.permissions);await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const result=await run(database);await flushFinanceDeferredEffects(database);const after=await targetSource(database,actor,command.target);
  if(command.expected&&(after.productId!==command.expected.productId||command.expected.warehouseId&&after.owner?.hold.warehouse_id!==command.expected.warehouseId||(command.expected.lotId??null)!==(after.owner?.hold.lot_id??null)))throw new ApiError(409,"QUALITY_SOURCE_CHANGED","The saved Quality product, warehouse or lot differs from the submitted source.");
  if(before&&before.witness!==after.witness)throw new ApiError(409,"QUALITY_SOURCE_CHANGED","The original Quality source, product or warehouse changed.");
  await current(database,actor,key,command.permissions);const observed=await database.batch(boundaries.flatMap(item=>[...item.boundary.after,clearWorkflowCommandAssertions(database, item.key)]));boundaries[0]?.boundary.observe?.(observed);return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
