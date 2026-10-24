import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards,financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {isTransactionalSqlDatabase,type SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {flushFinanceDeferredEffects} from "@erpx/server-finance/public/finance-transactions";
import {checkPurchasedCapability,readPurchasedAccesses,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure,commercePackageEnforcementMode,type PackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {commerceCompleteOwnerSourceReadLocks} from "@erpx/server-commerce/adapters/owner-source-locks";
import {ecommerceCommerceSourceReadLocks} from "@erpx/server-ecommerce/public/commerce-source-locks";
import {lockQueuedCommandAuthority} from "@erpx/server-identity_access/public/queued-command-authority";

type OrderKind="quote"|"sales"|"purchase";
export type CommercePurchaseActor=Pick<WorkContext,"userId"|"assignmentId"|"role"|"companyId"|"branchId"|"departmentId"|"authority"|"iam">;
type Source={id:string;company_id:string;branch_id:string|null;department_id:string|null;kind:OrderKind};
type Target={type:"order"|"fulfillment"|"return"|"invoice";id:string}|{type:"new";id:string;kind:OrderKind};
type Scope={companyId:string;capability:string;purpose:CapabilityPurpose};
export type CommercePurchaseCommand={operation:string;key:string;target:Target;purpose:CapabilityPurpose;permissions:readonly string[];
  kindAuthority?:boolean;transactional?:boolean;stock?:boolean;financial?:boolean;receiving?:boolean;convert?:boolean;stockLines?:readonly string[]};
type FinalCheck=()=>Promise<void>;
type Composition={companyId:string;mode:PackageEnforcementMode;checks:FinalCheck[]};
// Private server-only transaction identity. An unregistered supplied connection
// cannot return before its owning handoff's final purchase and source checks.
const compositions=new WeakMap<SqlDatabase,Composition>();
const capability=(kind:OrderKind)=>kind==="quote"?"sales.quotes":kind==="sales"?"sales.orders":"supply.purchasing";
// Adapter-only reuse keeps each shared surface on this owner's actual kind and
// fresh IAM decision. Raw source facts are never exposed through a public port.
export {capability as commerceKindCapability,source as commercePurchaseSource,requireCurrent as requireCommerceCurrentAuthority};

function scoped(actor:CommercePurchaseActor,row:Source){
 if(row.company_id!==actor.companyId||actor.branchId&&row.branch_id!==actor.branchId||actor.departmentId&&row.department_id!==actor.departmentId)
  throw new ApiError(404,"ORDER_NOT_FOUND","Order not found in this workspace.");
 return row;
}
async function source(database:SqlDatabase,actor:CommercePurchaseActor,target:Target):Promise<Source|null>{
 if(target.type==="new")return null;
 const join=target.type==="order"?"":target.type==="fulfillment"?"JOIN commerce_fulfillments purchase_source ON purchase_source.order_id=o.id":target.type==="return"?"JOIN commerce_returns purchase_source ON purchase_source.order_id=o.id":"JOIN commerce_fulfillments purchase_source ON purchase_source.order_id=o.id";
 const where=target.type==="order"?"o.id=?":target.type==="invoice"?"purchase_source.invoice_id=?":"purchase_source.id=?";
 const rows=(await database.prepare(`SELECT DISTINCT o.id,o.company_id,o.branch_id,o.department_id,o.kind FROM commerce_orders o ${join} WHERE ${where}`).bind(target.id).all<Source>()).results;
 if(rows.length>1)throw new ApiError(409,"COMMERCE_SOURCE_AMBIGUOUS","Reconcile this invoice's actual Commerce sources.");
 return rows[0]?scoped(actor,rows[0]):null;
}
function receiptTarget(operation:string,id:string,fallback:Target):Target{
 if(operation.startsWith("commerce.return."))return {type:"return",id};
 if(operation==="commerce.invoice.create")return {type:"fulfillment",id};
 if(operation==="commerce.invoice.settle")return {type:"invoice",id};
 return operation.startsWith("commerce.order.")||operation==="commerce.stock.reserve"?{type:"order",id}:fallback;
}
function sourceTarget(target:Target):Target{return target.type==="new"?{type:"order",id:target.id}:target;}
async function requireCurrent(database:SqlDatabase,actor:CommercePurchaseActor,permissions:readonly string[],record:Source|null,key:string){
 // Every permission is selected by this owner command, never an input label.
 const scope=record??{company_id:actor.companyId,branch_id:actor.branchId,department_id:actor.departmentId};
 for(const permission of permissions){
  const row=await database.prepare(`SELECT ${assignmentReadSql("commerce_scope",permission)} allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) commerce_scope`)
   .bind(actor.assignmentId,actor.userId,actor.role,scope.company_id,scope.branch_id,scope.department_id).first<{allowed:boolean}>();
  if(!row?.allowed)throw new ApiError(403,"COMMERCE_ACCESS_CHANGED","Choose a current authorized Commerce work role.");
 }
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}
async function currentSource(database:SqlDatabase,actor:WorkContext,target:Target,expected:Source){
 const current=await source(database,actor,target);
 if(!current||["id","company_id","branch_id","department_id","kind"].some(field=>current[field as keyof Source]!==expected[field as keyof Source]))
  throw new ApiError(409,"COMMERCE_SOURCE_CHANGED","The original Commerce source or workspace changed. Review it again.");
 return current;
}
async function hasStock(database:SqlDatabase,record:Source|null,command:CommercePurchaseCommand){
 if(!record)return false;
 if(command.stockLines){
  if(!command.stockLines.length)return false;
  return !!await database.prepare(`SELECT 1 FROM commerce_order_lines WHERE order_id=? AND item_type!='service' AND id IN (${command.stockLines.map(()=>"?").join(",")}) LIMIT 1`).bind(record.id,...command.stockLines).first();
 }
 if(command.target.type==="return")return !!await database.prepare("SELECT 1 FROM commerce_return_lines r JOIN commerce_fulfillment_lines f ON f.id=r.fulfillment_line_id JOIN commerce_order_lines l ON l.id=f.order_line_id WHERE r.return_id=? AND l.item_type!='service' LIMIT 1").bind(command.target.id).first();
 return command.stock===true;
}

/** Overview/options keep shared references, but its mixed order list must not
 * preload another product's documents or let hidden kinds consume its LIMIT. */
export async function commerceOrderReadKinds(database:SqlDatabase,actor:WorkContext){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return null;
 await requireCurrent(database,actor,["commerce.read"],null,`commerce-options-read:${randomUUID()}`);
 const kinds=["quote","sales","purchase"] as const,access=await readPurchasedAccesses(database,mode,actor.companyId,kinds.map(capability));
 return kinds.filter(kind=>access[capability(kind)].history);
}

export async function requireCommercePurchasedRead(database:SqlDatabase,actor:WorkContext,kind:OrderKind){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return;
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:capability(kind),purpose:"history"});
 await requireCurrent(database,actor,["commerce.read"],null,`commerce-read:${randomUUID()}`);
}
export async function requireCommerceOrderRead(database:SqlDatabase,actor:WorkContext,id:string){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return;
 const record=await source(database,actor,{type:"order",id});if(!record)throw new ApiError(404,"ORDER_NOT_FOUND","Order not found.");
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:capability(record.kind),purpose:"history"});
 await requireCurrent(database,actor,["commerce.read"],record,`commerce-record-read:${randomUUID()}`);
}

async function owned<Result>(owner:SqlDatabase,actor:WorkContext,mode:PackageEnforcementMode,run:(database:SqlDatabase)=>Promise<Result>,ecommerce=false){
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"TRANSACTION_REQUIRED","Commerce purchase decisions require a Holding transaction.");
 try{return await owner.transaction(async database=>{
  if(mode==="disabled")return run(database);
  const composition:Composition={companyId:actor.companyId,mode,checks:[]};compositions.set(database,composition);
  try{
   const locks=companyPurchaseReadLocks(database,actor.companyId);
   await database.batch([locks[0],...commerceCompleteOwnerSourceReadLocks(database),...(ecommerce?ecommerceCommerceSourceReadLocks(database):[])]);
   if(ecommerce)await lockQueuedCommandAuthority(database);
   await database.batch(locks.slice(1));
   const result=await run(database);
   await flushFinanceDeferredEffects(database);
   for(const check of composition.checks)await check();
   return result;
  }finally{compositions.delete(database);}
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
/** Ecommerce owns the complete handoff. Its original effects finish before the
 * Finance projection and queued source/current-authority/purchase checks. */
export async function commerceOwnerTransaction<Result>(owner:SqlDatabase,actor:WorkContext,run:(database:SqlDatabase)=>Promise<Result>){
 return owned(owner,actor,await commercePackageEnforcementMode(),run,true);
}

/** A named owner handoff joins this exact Commerce transaction. Its final
 * source checks run after every original deferred Finance/stock effect. */
export function deferCommerceOwnerCheck(database:SqlDatabase,actor:Pick<WorkContext,"companyId">,check:FinalCheck){
 const composition=compositions.get(database);
 if(!composition||composition.companyId!==actor.companyId)throw new ApiError(503,"COMMERCE_COMPOSITION_REQUIRED","The handoff requires its complete owned Commerce transaction.");
 composition.checks.push(check);
}

/** The original Commerce application owns every receipt/hash, workflow,
 * independence/version guard, audit/outbox and prepared Finance/stock effect. */
export async function withCommercePurchasedCommand<Result>(actor:WorkContext,command:CommercePurchaseCommand,run:(database:SqlDatabase)=>Promise<Result>,provided?:SqlDatabase){
 const mode=await commercePackageEnforcementMode(),owner=(await holdingResources(actor.holdingId)).write;
 if(mode==="disabled")return provided?run(provided):command.transactional?owned(owner,actor,mode,run):run(owner);
 const execute=async(database:SqlDatabase)=>{
  const composition=compositions.get(database);
  if(!composition||composition.companyId!==actor.companyId)throw new ApiError(503,"COMMERCE_COMPOSITION_REQUIRED","Commerce requires its complete owned Holding transaction.");
  await database.batch(companyPurchaseReadLocks(database,actor.companyId));
  const receipt=await database.prepare("SELECT operation,record_id FROM command_receipts WHERE command_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?")
   .bind(command.key,actor.userId,actor.companyId).first<{operation:string;record_id:string}>();
  const target=receipt?receiptTarget(receipt.operation,receipt.record_id,command.target):command.target;
  const record=await source(database,actor,target);
  if(!record&&target.type!=="new"){
   // A generic Finance/expense invoice has no actual Commerce fulfillment FK.
   // Its existing Finance owner remains responsible for purchase enforcement.
   if(target.type==="invoice")return run(database);
   throw new ApiError(404,"ORDER_NOT_FOUND","The original Commerce source was not found.");
  }
  const kind=record?.kind??(target.type==="new"?target.kind:null);
  if(!kind)throw new ApiError(404,"ORDER_NOT_FOUND","The original Commerce source was not found.");
  const permissions=[...new Set([...command.permissions,...(command.kindAuthority?[kind==="purchase"?"commerce.purchase":"commerce.sales"]:[]),...(command.operation==="commerce.invoice.settle"&&kind==="purchase"?["finance.pay"]:[])])];
  const purpose=receipt?"history":command.purpose;
  const scopes:Scope[]=[{companyId:actor.companyId,capability:capability(kind),purpose}];
  if(command.convert)scopes.push({companyId:actor.companyId,capability:"sales.orders",purpose});
  if(command.receiving&&kind==="purchase")scopes.push({companyId:actor.companyId,capability:"supply.receiving",purpose});
  if(await hasStock(database,record,command))scopes.push({companyId:actor.companyId,capability:kind==="purchase"?"supply.stock":"inventory.stock-core",purpose});
  if(command.financial)scopes.push({companyId:actor.companyId,capability:kind==="purchase"?"finance.inventory-valuation":"finance.sales-posting",purpose});
  const key=`commerce-purchase:${randomUUID()}`,boundaries=scopes.map((scope,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,mode,`${key}:${index}`,scope)!}));
  for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);
  await requireCurrent(database,actor,permissions,record,key);
  await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const result=await run(database),finalTarget=sourceTarget(target),after=await source(database,actor,finalTarget);
  if(!after||after.kind!==kind)throw new ApiError(409,"COMMERCE_SOURCE_CHANGED","The actual Commerce owner changed.");
  const check=async()=>{
   const current=await currentSource(database,actor,finalTarget,after);
   await requireCurrent(database,actor,permissions,current,key);
   if(command.convert&&!receipt)await database.batch([assertion(database,key,"commerce-conversion-source","EXISTS(SELECT 1 FROM commerce_orders WHERE id=? AND company_id=? AND source_id=? AND kind='sales')",[`order-${command.key}`,actor.companyId,current.id]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
   const observed=await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key)]));boundaries[0]?.boundary.observe?.(observed);
  };
  // Check now, and repeat after the outer owner's complete deferred effects.
  await check();composition.checks.push(check);return result;
 };
 if(provided){
  const ownership=owner as SqlDatabase&{assertActiveTransaction?:(database:SqlDatabase)=>void};
  if(!ownership.assertActiveTransaction)throw new ApiError(503,"TRANSACTION_OWNERSHIP_REQUIRED","Commerce requires an active owned Holding transaction.");
  ownership.assertActiveTransaction(provided);return execute(provided);
 }
 return owned(owner,actor,mode,execute);
}
