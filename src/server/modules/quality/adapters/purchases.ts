import "server-only";
import {qualityStockFacts} from "../../commerce/public/quality-stock";
import {qualityProductFacts} from "../../commerce/public/quality-product";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "../../identity_access/public/authorization";
import {hasPermission} from "@/lib/identity_access/permissions";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import {iamMutationGuards} from "../../identity_access/public/iam-store";
import {workContextGuards,financialConstraintError} from "../../workflow/public/commands";
import {commercePackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from "../../commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "../../commercial/public/rules/packages";
import {flushFinanceDeferredEffects} from "../../finance/public/finance-transactions";
import {isTransactionalSqlDatabase,type SqlDatabase} from "../../runtime/public/runtime/types";
import {holdingResources} from "../../runtime/public/runtime";
import {ApiError} from "../../runtime/public/http";
import {qualityHoldPurchaseSource,qualityPurchaseSourceReadLocks,type QualityPurchaseSource} from "./purchase-sources";
type Actor=Pick<WorkContext,"userId"|"assignmentId"|"role"|"companyId"|"branchId"|"departmentId"|"authority"|"iam">;
export type QualityPurchaseTarget={kind:"hold"|"spec"|"alert";id:string};
export type QualityPurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
const capabilities=["supply.quality","supply.receiving","supply.stock","sales.orders","inventory.stock-core"] as const;
async function current(database:SqlDatabase,actor:Actor,key:string,permissions:readonly string[]=[]){
 if(actor.departmentId)throw new ApiError(403,"QUALITY_ACCESS_CHANGED","Choose a current Company or Branch Quality assignment.");
 for(const permission of new Set(["quality.read",...permissions])){
  if(!hasPermission(actor,permission))throw new ApiError(403,"QUALITY_ACCESS_CHANGED","Choose a current authorized Quality assignment.");
  const row=await database.prepare(`SELECT ${assignmentReadSql("quality_scope",permission)} allowed FROM (SELECT ?::text company_id,?::text branch_id,NULL::text department_id) quality_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId).first<{allowed:boolean}>();
  if(!row?.allowed)throw new ApiError(403,"QUALITY_ACCESS_CHANGED","The original Quality assignment is no longer current.");
 }
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}
async function targetSource(database:SqlDatabase,actor:Actor,target:QualityPurchaseTarget):Promise<{capabilities:readonly string[];witness:string;owner?:QualityPurchaseSource;productId:string}>{
 if(target.kind==="spec"){
  const row=await database.prepare("SELECT id,company_id,product_id,created_by FROM inventory_quality_specs WHERE id=? AND company_id=?").bind(target.id,actor.companyId).first<{id:string;company_id:string;product_id:string;created_by:string}>();
  if(!row)throw new ApiError(404,"QUALITY_SPEC_NOT_FOUND","The original inspection specification is unavailable.");return {capabilities:["supply.quality"],witness:JSON.stringify(row),productId:row.product_id};
 }
 const id=target.kind==="alert"?(await database.prepare("SELECT hold_id FROM inventory_quality_alerts WHERE id=?").bind(target.id).first<{hold_id:string}>())?.hold_id:target.id;
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
  const replay=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,command.target.id,actor.userId,actor.companyId).first();
  const before=command.existing||replay?await targetSource(database,actor,command.target):null,purpose=replay?"history" as const:command.purpose;
  const required=before?.capabilities??["supply.quality"],extra=command.disposition&&before?.owner?.kind!=="customer-return"?["supply.counts"]:[],scopes=[...new Set([...required,...extra])].map(capability=>({companyId:actor.companyId,capability,purpose}));
  const key=`quality-purchase:${randomUUID()}`,boundaries=scopes.map((scope,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,mode,`${key}:${index}`,scope)!}));
  for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);await current(database,actor,key,command.permissions);await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const result=await run(database);await flushFinanceDeferredEffects(database);const after=await targetSource(database,actor,command.target);
  if(command.expected&&(after.productId!==command.expected.productId||command.expected.warehouseId&&after.owner?.hold.warehouse_id!==command.expected.warehouseId||(command.expected.lotId??null)!==(after.owner?.hold.lot_id??null)))throw new ApiError(409,"QUALITY_SOURCE_CHANGED","The saved Quality product, warehouse or lot differs from the submitted source.");
  if(purpose==="new-work"){
   if(after.owner){const stock=await qualityStockFacts(database,actor,{warehouseId:after.owner.hold.warehouse_id,productId:after.productId});if(!stock||stock.branch_id!==after.owner.hold.branch_id)throw new ApiError(409,"QUALITY_SOURCE_CHANGED","The Quality product or current warehouse policy changed.");}
   else if(!await qualityProductFacts(database,actor,after.productId))throw new ApiError(409,"QUALITY_SOURCE_CHANGED","The original specification product is no longer active.");
  }
  if(before&&before.witness!==after.witness)throw new ApiError(409,"QUALITY_SOURCE_CHANGED","The original Quality source, product or warehouse changed.");
  await current(database,actor,key,command.permissions);const observed=await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key)]));boundaries[0]?.boundary.observe?.(observed);return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}

/** Actor-free durable escalation stays with the actual existing hold. Normal
 * recipient SQL still checks current roles; no management package is imposed
 * on shared quarantine facts or another product's unrelated Stock jobs. */
export async function withQualityPurchasedSweep<Result>(owner:SqlDatabase,run:(database:SqlDatabase,scope?:{companyId:string;holdIds:string[]})=>Promise<Result>,merge:(results:Result[])=>Result){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return run(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"QUALITY_TRANSACTION_REQUIRED","Quality escalation requires its owned Holding transaction.");
 const companies=(await owner.prepare("SELECT DISTINCT company_id FROM inventory_quality_holds WHERE status NOT IN ('released','cancelled','disposed') ORDER BY company_id").all<{company_id:string}>()).results,results:Result[]=[];
 for(const company of companies){results.push(await owner.transaction(async database=>{
  const locks=companyPurchaseReadLocks(database,company.company_id);await database.batch([locks[0],...qualityPurchaseSourceReadLocks(database),...locks.slice(1)]);
  const access=await readPurchasedAccesses(database,mode,company.company_id,capabilities),ids=(await database.prepare("SELECT id FROM inventory_quality_holds WHERE company_id=? AND status NOT IN ('released','cancelled','disposed') ORDER BY id").bind(company.company_id).all<{id:string}>()).results;
  const sources:QualityPurchaseSource[]=[];for(const row of ids){const source=await qualityHoldPurchaseSource(database,{companyId:company.company_id,branchId:null},row.id);if(source.capabilities.every(capability=>access[capability].settleExisting))sources.push(source);}
  const required=[...new Set(sources.flatMap(source=>source.capabilities))],boundaries=required.map(capability=>{const key=`quality-sweep:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,{companyId:company.company_id,capability,purpose:"settle-existing"})!};});
  await database.batch(boundaries.flatMap(item=>item.boundary.before));const result=await run(database,{companyId:company.company_id,holdIds:sources.map(source=>source.hold.id)});await flushFinanceDeferredEffects(database);
  for(const source of sources)if((await qualityHoldPurchaseSource(database,{companyId:company.company_id,branchId:null},source.hold.id)).witness!==source.witness)throw new ApiError(409,"QUALITY_SOURCE_CHANGED","The original escalation source changed before commit.");
  const observed=await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key)]));boundaries[0]?.boundary.observe?.(observed);return result;
 }));}
 return merge(results);
}
