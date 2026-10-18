import "server-only";
import {randomUUID} from "node:crypto";
import type {FlowView} from "@/lib/shared/flow";
import {hasPermission} from "@/lib/identity_access/permissions";
import {ApiError} from "../../runtime/public/http";
import {isTransactionalSqlDatabase,type SqlDatabase} from "../../runtime/public/runtime/types";
import {checkPurchasedCapability,companyPurchaseReadLocks,commercePackageEnforcementMode,purchaseCommandBoundary,purchaseCommandFailure} from "../../commercial/public/purchase-command-boundary";
import {flushFinanceDeferredEffects} from "../../finance/public/finance-transactions";
import {financialConstraintError} from "../../workflow/public/commands";
import {commerceKindCapability,commercePurchaseSource,requireCommerceCurrentAuthority,type CommercePurchaseActor} from "./purchase-boundary";
import {commerceWorkAccess} from "./record-purchases";
import {commerceCompleteOwnerSourceReadLocks} from "./owner-source-locks";

/** The generic Flow API supplies its original minimal actor and trusted Holding
 * pool. A product label or a transaction client cannot manufacture ownership. */
export async function requireCommerceFlowHistory(database:SqlDatabase,actor:CommercePurchaseActor,id:string){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return;
 const source=await commercePurchaseSource(database,actor,{type:"order",id});
 if(!source)throw new ApiError(404,"FLOW_NOT_FOUND","Workflow document not found in this workspace.");
 if(!hasPermission(actor,"commerce.read"))throw new ApiError(403,"COMMERCE_ACCESS_CHANGED","Choose a current authorized Commerce work role.");
 await requireCommerceCurrentAuthority(database,actor,["commerce.read"],source,`commerce-flow-read:${randomUUID()}`);
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:commerceKindCapability(source.kind),purpose:"history"});
}
export async function commerceDocumentFlow(database:SqlDatabase,actor:CommercePurchaseActor,view:FlowView){
 const access=await commerceWorkAccess(database,actor,[view.recordId]);
 if(access===null)return view;
 const source=access.get(view.recordId);
 if(!source?.history)throw new ApiError(403,"PACKAGE_REQUIRED","This Commerce document requires purchased access or retained history.");
 return source.canAct?view:{...view,canAct:false,canManage:false};
}

export async function withCommerceFlowAssignment<Result>(owner:SqlDatabase,actor:CommercePurchaseActor,id:string,
 command:{key:string;stepKey:string;saveDefault:boolean},run:(database:SqlDatabase)=>Promise<Result>){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return run(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"COMMERCE_TRANSACTION_REQUIRED","Commerce Flow requires its owned Holding transaction.");
 try{return await owner.transaction(async database=>{
  const locks=companyPurchaseReadLocks(database,actor.companyId);
  await database.batch([locks[0],...commerceCompleteOwnerSourceReadLocks(database),...locks.slice(1)]);
  const order=await database.prepare("SELECT state,version FROM commerce_orders WHERE company_id=? AND id=? FOR SHARE").bind(actor.companyId,id).first<{state:string;version:number}>();
  const before=await commercePurchaseSource(database,actor,{type:"order",id});
  if(!before)throw new ApiError(404,"FLOW_NOT_FOUND","Workflow document not found in this workspace.");
  if(!hasPermission(actor,"commerce.read"))throw new ApiError(403,"COMMERCE_ACCESS_CHANGED","Choose a current authorized Commerce work role.");
  const replay=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation='flow.assign' AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?")
   .bind(command.key,id,actor.userId,actor.companyId).first();
  const newWork=command.saveDefault||["draft","submitted"].includes(order?.state??"")||before.kind==="quote"&&order?.state==="confirmed"
   ||["draft","submitted"].includes(command.stepKey)||before.kind==="quote"&&command.stepKey==="confirmed";
  const scope={companyId:actor.companyId,capability:commerceKindCapability(before.kind),purpose:replay?"history" as const:newWork?"new-work" as const:"settle-existing" as const};
  const key=`commerce-flow:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,scope)!;
  await checkPurchasedCapability(database,mode,scope);
  await requireCommerceCurrentAuthority(database,actor,["commerce.read"],before,key);
  await database.batch(boundary.before);
  const result=await run(database);
  await flushFinanceDeferredEffects(database);
  const after=await commercePurchaseSource(database,actor,{type:"order",id});
  const current=await database.prepare("SELECT state,version FROM commerce_orders WHERE company_id=? AND id=?").bind(actor.companyId,id).first<{state:string;version:number}>();
  if(!after||!current||order?.state!==current.state||order.version!==current.version||Object.keys(before).some(field=>before[field as keyof typeof before]!==after[field as keyof typeof after]))
   throw new ApiError(409,"COMMERCE_SOURCE_CHANGED","The original Commerce source or workspace changed. Review it again.");
  await requireCommerceCurrentAuthority(database,actor,["commerce.read"],after,key);
  const observations=await database.batch([...boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
  boundary.observe?.(observations);return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
