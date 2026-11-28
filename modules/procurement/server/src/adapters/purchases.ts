import {commerceCompleteOwnerSourceReadLocks} from "@erpx/commerce-server/public/owner-source-locks";
import {procurementCommandSourceImage,rfqLinesAgg,rfqOffersAgg,rfqOfferLinesAgg} from "@erpx/procurement-server/adapters/command-source";
import {commandReplayGuard} from './read-tail-sql';
import {assignmentScopeProbe} from './read-tail-r30';
import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {iamMutationGuards} from "@erpx/identity-access-server/public/iam-store";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {workContextGuards,financialConstraintError} from '@erpx/workflow-server/public/command-guards';
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {isTransactionalSqlDatabase,type SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {flushFinanceDeferredEffects} from "@erpx/finance-server/public/finance-transactions";
import {deferCommerceOwnerCheck} from "@erpx/commerce-server/public/purchase-boundary";
import {commercePackageEnforcementMode,checkPurchasedCapability,readPurchasedAccess,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from "@erpx/commercial-server/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/commercial-server/public/rules/packages";
import {clearWorkflowCommandAssertions} from "@erpx/workflow-server/public/command-assertion-effects";

type Target={kind:"rfq"|"purchase";id:string};
type Source={id:string;company_id:string;branch_id:string|null;department_id:string|null;image:string};
type Command={key:string;operation:string;purpose:CapabilityPurpose;permissions:readonly string[];target:Target;creating?:boolean};
const capability="supply.purchasing";
const table=(target:Target)=>target.kind==="rfq"?"procurement_rfqs":"commerce_orders";
const actual=(target:Target)=>target.kind==="purchase"?" AND r.kind='purchase'":"";

async function source(database:SqlDatabase,actor:WorkContext,target:Target,lock=false):Promise<Source|null>{
 const row=await database.prepare(`SELECT r.id,r.company_id,r.branch_id,r.department_id,(${procurementCommandSourceImage(target.kind,"r")})::text image FROM ${table(target)} r WHERE r.id=?${actual(target)}${lock?" FOR UPDATE":""}`).bind(target.id).first<Source>();
 if(!row||row.company_id!==actor.companyId||actor.branchId&&row.branch_id!==actor.branchId||actor.departmentId&&row.department_id!==actor.departmentId)return null;
 return row;
}
async function current(database:SqlDatabase,actor:WorkContext,permissions:readonly string[],record:Source|null,key:string){
 const scope=record??{company_id:actor.companyId,branch_id:actor.branchId,department_id:actor.departmentId};
 for(const permission of permissions){
  const allowed=await database.prepare(assignmentScopeProbe("procurement_scope",permission))
   .bind(actor.assignmentId,actor.userId,actor.role,scope.company_id,scope.branch_id,scope.department_id).first<{allowed:boolean}>();
  if(!allowed?.allowed)throw new ApiError(403,"PROCUREMENT_ACCESS_CHANGED","Choose a current authorized purchasing work role.");
 }
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),clearWorkflowCommandAssertions(database, key)]);
}
function sourceFence(database:SqlDatabase,target:Target,record:Source,key:string){
 return [assertion(database,key,"procurement-source",`EXISTS(SELECT 1 FROM ${table(target)} r WHERE r.id=? AND (${procurementCommandSourceImage(target.kind,"r")})=?::jsonb${actual(target)})`,[record.id,record.image]),clearWorkflowCommandAssertions(database, key)];
}
function sourceFailure(error:unknown){
 const failure=error as {code?:string;detail?:string},detail=failure?.detail;
 if(failure?.code!=="23514"||!detail||!["procurement-source","rfq-handoff-source"].some(step=>detail.includes(step)))return null;
 const result=new ApiError(409,"PROCUREMENT_SOURCE_CHANGED","The purchasing source changed before commit. Refresh its workflow and retry.");result.cause=error;return result;
}
export async function procurementPurchaseAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return undefined;
 await current(database,actor,["commerce.read"],null,`procurement-read:${randomUUID()}`);
 return readPurchasedAccess(database,mode,{companyId:actor.companyId,capability});
}
export async function requireProcurementPurchasedRead(database:SqlDatabase,actor:WorkContext){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return undefined;
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability,purpose:"history"});
 return procurementPurchaseAccess(database,actor);
}

/** RFQ and promises retain their original receipts, independent review and
 * optimistic versions. All effects finish in this same Holding transaction. */
export async function withProcurementPurchasedCommand<Result>(actor:WorkContext,command:Command,run:(database:SqlDatabase)=>Promise<Result>){
 const owner=(await holdingResources(actor.holdingId)).write,mode=await commercePackageEnforcementMode();if(mode==="disabled")return run(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"TRANSACTION_REQUIRED","Purchasing requires its original Holding transaction.");
 try{return await owner.transaction(async database=>{
  const companyLocks=companyPurchaseReadLocks(database,actor.companyId);
  await database.batch([companyLocks[0],...commerceCompleteOwnerSourceReadLocks(database),...companyLocks.slice(1)]);
  const receipt=await database.prepare(commandReplayGuard).bind(command.key,command.operation,actor.userId,actor.companyId).first();
  const scope={companyId:actor.companyId,capability,purpose:receipt?"history" as const:command.purpose};
  await checkPurchasedCapability(database,mode,scope);
  const before=await source(database,actor,command.target,true);
  if(!before&&!command.creating)throw new ApiError(404,"PROCUREMENT_SOURCE_NOT_FOUND","The original purchasing source was not found in this workspace.");
  const key=`procurement-purchase:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,scope)!;
  await current(database,actor,command.permissions,before,key);await database.batch(boundary.before);
  const result=await run(database),after=await source(database,actor,command.target);
  if(!after)throw new ApiError(409,"PROCUREMENT_SOURCE_CHANGED","The original purchasing source changed.");
  await flushFinanceDeferredEffects(database);
  await current(database,actor,command.permissions,after,key);
  const observed=await database.batch([...sourceFence(database,command.target,after,key),...boundary.after,clearWorkflowCommandAssertions(database, key)]);boundary.observe?.(observed);
  return result;
 });}catch(error){throw sourceFailure(error)??purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}

/** Commerce owns the real PO. Register a source fence with that exact private
 * composition, so no RFQ check returns before the outer deferred effects. */
export async function prepareRfqPurchaseHandoff(database:SqlDatabase,actor:WorkContext,rfqId:string,orderId:string,key:string,editing:boolean){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return;
 await database.batch(companyPurchaseReadLocks(database,actor.companyId));
 const target:Target={kind:"rfq",id:rfqId},before=await source(database,actor,target,true);
 if(!before)throw new ApiError(404,"RFQ_NOT_FOUND","Quotation request not found in this scope.");
 const scope={companyId:actor.companyId,capability,purpose:"new-work" as const},boundaryKey=`rfq-handoff-purchase:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,boundaryKey,scope)!;
 await checkPurchasedCapability(database,mode,scope);await current(database,actor,["commerce.read","commerce.purchase"],before,boundaryKey);await database.batch(boundary.before);
 const sourceImage=JSON.parse(before.image) as {record:Record<string,unknown>;lines:unknown;offers:unknown;offerLines:unknown};
 const original=sourceImage.record;
 const expected:Record<string,unknown>={...original,...(editing?{}:{status:"ordered",order_id:orderId,version:Number(original.version)+1})};delete expected.updated_at;
 deferCommerceOwnerCheck(database,actor,async()=>{
  await current(database,actor,["commerce.read","commerce.purchase"],before,boundaryKey);
  try{const observed=await database.batch([
   assertion(database,boundaryKey,"rfq-handoff-source",`EXISTS(SELECT 1 FROM procurement_rfqs r JOIN commerce_orders o ON o.id=r.order_id AND o.company_id=r.company_id AND o.kind='purchase' WHERE r.id=? AND o.id=? AND jsonb_build_object('record',to_jsonb(r)-'updated_at','lines',${rfqLinesAgg},'offers',${rfqOffersAgg},'offerLines',${rfqOfferLinesAgg})=?::jsonb)`,[rfqId,orderId,JSON.stringify({record:expected,lines:sourceImage.lines,offers:sourceImage.offers,offerLines:sourceImage.offerLines})]),
   ...boundary.after,clearWorkflowCommandAssertions(database, boundaryKey)
  ]);boundary.observe?.(observed);}catch(error){throw sourceFailure(error)??error;}
 });
}
