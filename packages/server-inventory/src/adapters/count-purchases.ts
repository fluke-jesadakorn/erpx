import {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import {serviceInitialPhysicalStockCountMarked,serviceCountPersistedMarker,serviceInitialPhysicalStockCapabilities,requireServiceInitialPhysicalStockHistory} from '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source';
import {posInitialPhysicalStockCountMarked,posInitialPhysicalStockCapabilities,requirePosInitialPhysicalStockHistory,withPosInitialPhysicalStockCountUnit} from '@erpx/server-inventory/adapters/pos-initial-physical-stock-count-source';
import {manufacturingInitialPhysicalStockCountMarked,manufacturingInitialPhysicalStockCapabilities,requireManufacturingInitialPhysicalStockHistory,withManufacturingInitialPhysicalStockCountUnit} from '@erpx/server-inventory/adapters/manufacturing-initial-physical-stock-count-source';
import "server-only";
import {initialPhysicalStockCountMarked,initialPhysicalStockCapabilities,requireInitialPhysicalStockHistory,withInitialPhysicalStockCountUnit} from "@erpx/server-inventory/adapters/initial-physical-stock-count-source";
import {qualityDisposalCountSource} from "@erpx/server-inventory/adapters/quality-count-purchases";
import {qualityPurchaseSourceReadLocks} from "@erpx/server-quality/public/purchase-sources";
import {randomUUID} from "node:crypto";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {FlowView} from "@erpx/feature-shared/lib/flow";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {workContextGuards,financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {commercePackageEnforcementMode,checkPurchasedCapability,readPurchasedAccesses,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {flushFinanceDeferredEffects} from "@erpx/server-finance/public/finance-transactions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {isTransactionalSqlDatabase,type SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";

type Actor=Pick<WorkContext,"userId"|"assignmentId"|"role"|"companyId"|"branchId"|"departmentId"|"authority"|"iam">;
type Source={id:string;company_id:string;branch_id:string|null;warehouse_id:string};
async function source(database:SqlDatabase,actor:Actor,id:string){
 const row=await database.prepare("SELECT id,company_id,branch_id,warehouse_id FROM inventory_counts WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)")
  .bind(id,actor.companyId,actor.branchId,actor.branchId).first<Source>();
 if(!row)throw new ApiError(404,"COUNT_NOT_FOUND","Stock count not found in this workspace.");
 return row;
}
async function current(database:SqlDatabase,actor:Actor,key:string,permission:string){
 if(actor.departmentId||!hasPermission(actor,permission))throw new ApiError(403,"COUNT_ACCESS_CHANGED","Choose a current inventory assignment.");
 const assignment=await database.prepare(`SELECT ${assignmentReadSql("count_scope",permission)} allowed FROM (SELECT ?::text company_id,?::text branch_id,NULL::text department_id) count_scope`)
  .bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId).first<{allowed:boolean}>();
 if(!assignment?.allowed)throw new ApiError(403,"COUNT_ACCESS_CHANGED","Choose a current inventory assignment.");
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}

/** Count management is supply.counts; shared Stock posting/reservation facts
 * consumed by POS, Hotel, Sales or Manufacturing do not acquire this gate. */
export async function countSourceAccess(database:SqlDatabase,actor:Actor,id?:string){
 if(id&&await serviceInitialPhysicalStockCountMarked(database,id)){await requireServiceInitialPhysicalStockHistory(database,actor as WorkContext,id);const all=await readPurchasedAccesses(database,'enforced',actor.companyId,serviceInitialPhysicalStockCapabilities);return {newWork:serviceInitialPhysicalStockCapabilities.every(cap=>all[cap].newWork),settleExisting:serviceInitialPhysicalStockCapabilities.every(cap=>all[cap].settleExisting),history:true,revision:all[serviceInitialPhysicalStockCapabilities[0]].revision};}
 if(id&&await posInitialPhysicalStockCountMarked(database,id)){await requirePosInitialPhysicalStockHistory(database,actor as WorkContext,id);const all=await readPurchasedAccesses(database,'enforced',actor.companyId,posInitialPhysicalStockCapabilities);return {newWork:posInitialPhysicalStockCapabilities.every(cap=>all[cap].newWork),settleExisting:posInitialPhysicalStockCapabilities.every(cap=>all[cap].settleExisting),history:true,revision:all[posInitialPhysicalStockCapabilities[0]].revision};}
 if(id&&await manufacturingInitialPhysicalStockCountMarked(database,id)){
  await requireManufacturingInitialPhysicalStockHistory(database,actor as WorkContext,id);
  const all=await readPurchasedAccesses(database,'enforced',actor.companyId,manufacturingInitialPhysicalStockCapabilities);
  return {newWork:manufacturingInitialPhysicalStockCapabilities.every(cap=>all[cap].newWork),settleExisting:manufacturingInitialPhysicalStockCapabilities.every(cap=>all[cap].settleExisting),history:true,revision:all[manufacturingInitialPhysicalStockCapabilities[0]].revision};
 }

 if(id&&await initialPhysicalStockCountMarked(database,id)){
  await requireInitialPhysicalStockHistory(database,actor as WorkContext,id);
  const all=await readPurchasedAccesses(database,'enforced',actor.companyId,initialPhysicalStockCapabilities);
  return {newWork:initialPhysicalStockCapabilities.every(cap=>all[cap].newWork),settleExisting:initialPhysicalStockCapabilities.every(cap=>all[cap].settleExisting),history:true,revision:all[initialPhysicalStockCapabilities[0]].revision};
 }
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return undefined;
 await current(database,actor,`count-read:${randomUUID()}`,"inventory.read");
 const quality=id?await qualityDisposalCountSource(database,actor,id):null,all=await readPurchasedAccesses(database,mode,actor.companyId,["supply.counts",...(quality?.capabilities??[])]),access=all["supply.counts"];
 return {newWork:access.newWork,settleExisting:access.settleExisting||!!quality&&quality.capabilities.every(capability=>all[capability].settleExisting),history:access.history||!!quality&&quality.capabilities.every(capability=>all[capability].history),revision:access.revision};
}
export async function countPurchaseControls(actor:WorkContext,id?:string){return countSourceAccess((await holdingResources(actor.holdingId)).write,actor,id);}
export async function requireCountPurchasedRead(database:SqlDatabase,actor:Actor,id?:string){
 const access=await countSourceAccess(database,actor,id);
 if(access&&!access.history)throw new ApiError(403,"PACKAGE_REQUIRED","Stock counts require the Supply purchase or retained history.");
 if(access&&id)await source(database,actor,id);
 return access;
}
export async function countDocumentFlow(database:SqlDatabase,actor:Actor,view:FlowView){
 const access=await requireCountPurchasedRead(database,actor,view.recordId);
 const canAct=!access||(view.state==="draft"||view.state==="rejected"?access.newWork:access.settleExisting);
 return canAct?view:{...view,canAct:false,canManage:false};
}

/** The original count, evidence, quantity/valuation, independent review and
 * receipt effects remain inside their complete Holding transaction. */
export async function withCountPurchasedCommand<Result>(owner:SqlDatabase,actor:Actor,
 command:{key:string;operation:string;id:string;purpose:CapabilityPurpose;permission:string;financial?:boolean;existing?:boolean;flowStep?:string;saveDefault?:boolean},run:(database:SqlDatabase)=>Promise<Result>){
 const work=actor as WorkContext;
 if(!work.holdingId)throw new ApiError(503,'COUNT_TRANSACTION_REQUIRED','Stock counts require the owned Holding transaction.');
 const guardedRun=async(database:SqlDatabase)=>{
  const {write}=await holdingResources(work.holdingId);if(!(write instanceof PostgresDatabase))throw new ApiError(503,'COUNT_TRANSACTION_REQUIRED','Stock counts require the owned Holding transaction.');write.assertActiveTransaction(database);
  if(await serviceCountPersistedMarker(database,command.id))throw new ApiError(404,'COUNT_NOT_FOUND','Stock count not found in this workspace.');
  return run(database);
 };
 if(await serviceCountPersistedMarker(owner,command.id))throw new ApiError(404,'COUNT_NOT_FOUND','Stock count not found in this workspace.');
 if(await posInitialPhysicalStockCountMarked(owner,command.id))return withPosInitialPhysicalStockCountUnit(owner,actor as WorkContext,{...command,id:command.id},async database=>{if(['inventory.count.create','inventory.count.update'].includes(command.operation)||command.saveDefault)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability:'supply.counts',purpose:'new-work'});return guardedRun(database);});
 if(await manufacturingInitialPhysicalStockCountMarked(owner,command.id))return withManufacturingInitialPhysicalStockCountUnit(owner,actor as WorkContext,{...command,id:command.id},async database=>{
  if(['inventory.count.create','inventory.count.update'].includes(command.operation)||command.saveDefault)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability:'supply.counts',purpose:'new-work'});
  return guardedRun(database);
 });
 // Shape detection authorizes nothing. The strict original-source unit repeats
 // actual scope/current authority and native source after query-free leases.
 if(await initialPhysicalStockCountMarked(owner,command.id))return withInitialPhysicalStockCountUnit(owner,actor as WorkContext,{...command,id:command.id},async database=>{
  if(['inventory.count.create','inventory.count.update'].includes(command.operation)||command.saveDefault)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability:'supply.counts',purpose:'new-work'});
  return guardedRun(database);
 });
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return isTransactionalSqlDatabase(owner)?owner.transaction(guardedRun):guardedRun(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"COUNT_TRANSACTION_REQUIRED","Stock counts require the owned Holding transaction.");
 try{return await owner.transaction(async database=>{
  const purchaseLocks=companyPurchaseReadLocks(database,actor.companyId);
  await database.batch(command.existing?[purchaseLocks[0],...qualityPurchaseSourceReadLocks(database),...purchaseLocks.slice(1)]:purchaseLocks);
  const replay=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?")
   .bind(command.key,command.operation,command.id,actor.userId,actor.companyId).first();
  const before=command.existing||replay?await source(database,actor,command.id):null;
  const flow=command.flowStep?await database.prepare("SELECT status FROM inventory_counts WHERE id=? AND company_id=?").bind(command.id,actor.companyId).first<{status:string}>():null;
  const flowNewWork=command.flowStep&&(command.saveDefault||["draft","rejected"].includes(flow?.status??"")||command.flowStep==="draft");
  const purpose=replay?"history" as const:flowNewWork?"new-work" as const:command.purpose;
  const quality=before&&purpose!=="new-work"?await qualityDisposalCountSource(database,actor,command.id):null;
  const baseAccess=quality?await readPurchasedAccesses(database,mode,actor.companyId,["supply.counts"]):null;
  const included=quality&&baseAccess?.["supply.counts"].codes[purpose]!=="GRANTED"?quality:null;
  const scopes=[...(included?.capabilities??["supply.counts"]).map(capability=>({companyId:actor.companyId,capability,purpose})),...(command.financial?[{companyId:actor.companyId,capability:included?.financeCapability??"finance.inventory-valuation",purpose}]:[])];
  const key=`count-purchase:${randomUUID()}`,boundaries=scopes.map((scope,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,mode,`${key}:${index}`,scope)!}));
  for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);
  await current(database,actor,key,command.permission);
  await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const result=await guardedRun(database);
  await flushFinanceDeferredEffects(database);
  const after=await source(database,actor,command.id);
  if(before&&Object.keys(before).some(field=>before[field as keyof Source]!==after[field as keyof Source]))throw new ApiError(409,"COUNT_SOURCE_CHANGED","The original count workspace or warehouse changed.");
  if(included&&(await qualityDisposalCountSource(database,actor,command.id))?.witness!==included.witness)throw new ApiError(409,"COUNT_SOURCE_CHANGED","The actual retained Quality disposal source changed.");
  await current(database,actor,key,command.permission);
  const observations=await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key)]));
  boundaries[0]?.boundary.observe?.(observations);return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
