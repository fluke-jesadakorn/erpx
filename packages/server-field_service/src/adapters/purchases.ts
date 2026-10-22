import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {fieldServiceTechnicianPredicate} from "@erpx/server-identity_access/public/field-service-authority";
import {workContextGuards,financialConstraintError} from "@erpx/server-workflow/public/commands";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {isTransactionalSqlDatabase,type SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {flushFinanceDeferredEffects} from "@erpx/server-finance/public/finance-transactions";
import {fieldServiceCustomerSourceRows,fieldServiceFinanceSourceReadLocks} from "@erpx/server-finance/public/field-service-references";
import {fieldServiceAssetSourceRows,fieldServiceAssetSourceReadLocks} from "@erpx/server-assets/public/field-service-references";
import {fieldServiceTicketSourceRows,fieldServiceTicketSourceReadLocks} from "@erpx/server-support/public/field-service-sources";
import {fieldServiceInventorySourceReadLocks} from "@erpx/server-inventory/public/field-service-purchase-sources";
import {servicePackageEnforcementMode,companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from "@erpx/server-commercial/public/purchase-command-boundary";
import {serviceCommandPurchases} from "@erpx/server-field_service/domain/package-commands";

export type ServiceOwnerTarget={kind:"agreement"|"order";id:string};
type Target=ServiceOwnerTarget;
type Command={key:string;operation:string;target:Target;creating?:boolean;asset?:boolean;sources?:readonly Target[]};
type Source={image:string;identity:string;asset:boolean;state:string};
type Composition={companyId:string;assignmentId:string;command:Command;replay:boolean;checks:(()=>Promise<void>)[]};
const compositions=new WeakMap<SqlDatabase,Composition>();
const scope="r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)";

/** Closed source identity; mutable visit and case projection fields stay out
 * of the before/after identity comparison, but the final image retains the
 * exact post-command Service state and actual reference eligibility. */
export function serviceOwnerSourceQuery(target:Target,file=false){
 const agreement=target.kind==="agreement"?"r":"g",record=target.kind==="agreement"?"field_service_agreements":"field_service_orders";
 const identity=target.kind==="agreement"?"to_jsonb(r)-'status'-'version'-'updated_at'-'reviewed_by'-'reviewed_at'":"jsonb_build_array(r.id,r.company_id,r.branch_id,r.department_id,r.ticket_id,r.ticket_version,r.agreement_id,r.agreement_version,r.asset_id,r.created_by,r.created_at)";
 return `SELECT jsonb_build_object('record',${file?"to_jsonb(r)-'version'-'updated_at'":"to_jsonb(r)"},'agreement',to_jsonb(${agreement}),'customer',to_jsonb(customer),'asset',to_jsonb(asset)${target.kind==="order"?`, 'ticket',to_jsonb(ticket),'technicianEligible',${fieldServiceTechnicianPredicate("r")},'parts',COALESCE((SELECT jsonb_agg(to_jsonb(part) ORDER BY part.id) FROM field_service_part_reservations part WHERE part.order_id=r.id),'[]'::jsonb),'billing',COALESCE((SELECT jsonb_agg(to_jsonb(billing) ORDER BY billing.id) FROM field_service_billing_releases billing WHERE billing.order_id=r.id),'[]'::jsonb)`:""})::text image,
 jsonb_build_object('record',${identity},'customer',to_jsonb(customer),'asset',to_jsonb(asset)${target.kind==="order"?",'agreement',to_jsonb(g),'ticket',to_jsonb(ticket)":""})::text identity,
 (${agreement}.asset_id IS NOT NULL${target.kind==="order"?" OR r.asset_id IS NOT NULL":""}) asset,r.status state
 FROM ${record} r ${target.kind==="order"?"JOIN field_service_agreements g ON g.id=r.agreement_id AND g.company_id=r.company_id AND g.branch_id=r.branch_id AND g.department_id IS NOT DISTINCT FROM r.department_id":""}
 JOIN (${fieldServiceCustomerSourceRows}) customer ON customer.id=${agreement}.customer_id AND customer.company_id=r.company_id AND customer.kind='customer'
 LEFT JOIN (${fieldServiceAssetSourceRows}) asset ON asset.id=r.asset_id AND asset.company_id=r.company_id AND asset.branch_id=r.branch_id
 ${target.kind==="order"?`JOIN (${fieldServiceTicketSourceRows}) ticket ON ticket.id=r.ticket_id AND ticket.company_id=r.company_id AND ticket.customer_id=g.customer_id AND ticket.branch_id=r.branch_id AND ticket.department_id IS NOT DISTINCT FROM r.department_id`:""}
 WHERE r.id=? AND ${scope} AND (r.asset_id IS NULL OR asset.id IS NOT NULL)`;
}
const sourceArgs=(actor:WorkContext,target:Target)=>[target.id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId];
export async function serviceOwnerSourceImage(database:SqlDatabase,actor:WorkContext,target:Target,file=false){return database.prepare(serviceOwnerSourceQuery(target,file)).bind(...sourceArgs(actor,target)).first<Source>();}
const source=serviceOwnerSourceImage;
async function requireSource(database:SqlDatabase,actor:WorkContext,target:Target){const value=await source(database,actor,target);if(!value)throw new ApiError(404,target.kind==="agreement"?"FIELD_SERVICE_AGREEMENT_NOT_FOUND":"FIELD_SERVICE_ORDER_NOT_FOUND","The original Service source is not available in this workspace.");return value;}
async function current(database:SqlDatabase,actor:WorkContext,key:string,review=false,write=true){
 for(const permission of ["records.read",...(write?["records.write"]:[]),...(review?["records.review"]:[])]){
  const allowed=await database.prepare(`SELECT ${assignmentReadSql("service_scope",permission)} allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) service_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first<boolean>("allowed");
  if(!allowed)throw new ApiError(403,"WORK_ROLE_UNAVAILABLE","The original Service assignment is no longer current.");
 }
 const selected=await database.prepare("SELECT jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> '[\"field_service\"]'::jsonb allowed FROM assignments WHERE id=?").bind(actor.assignmentId).first<boolean>("allowed");
 if(!actor.modules.includes("field_service")||!selected)throw new ApiError(403,"MODULE_FORBIDDEN","The current assignment no longer selects Field Service.");
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}
export function requireServiceCurrentAuthority(database:SqlDatabase,actor:WorkContext,key:string,write=false){return current(database,actor,key,false,write);}
export function serviceOwnerSourceReadLocks(database:SqlDatabase,companyId:string,stock=false){const locks=companyPurchaseReadLocks(database,companyId);return [locks[0],...fieldServiceTicketSourceReadLocks(database),...fieldServiceAssetSourceReadLocks(database),...fieldServiceFinanceSourceReadLocks(database),...(stock?fieldServiceInventorySourceReadLocks(database):[]),database.prepare("LOCK TABLE field_service_agreements,field_service_agreement_events,field_service_orders,field_service_order_events,field_service_part_reservations,field_service_billing_releases IN SHARE ROW EXCLUSIVE MODE"),...locks.slice(1)];}
const needsReview=(operation:string)=>operation.startsWith("field.agreement.")&&operation!=="field.agreement.create"||operation==="field.order.release-billing";

/** Only the original owner may register this transaction. Raw supplied pool
 * handles or unrelated handoff connections never become trusted compositions. */
export async function withServicePurchasedCommand<Result>(actor:WorkContext,describe:()=>Command,run:(database?:SqlDatabase)=>Promise<Result>){
 const mode=await servicePackageEnforcementMode();if(mode==="disabled")return run();
 const command=describe(),owner=(await holdingResources(actor.holdingId)).write;
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"SERVICE_TRANSACTION_REQUIRED","Use the original owned Service Holding transaction.");
 try{return await owner.transaction(async database=>{
  await database.batch(serviceOwnerSourceReadLocks(database,actor.companyId,["field.order.reserve-part","field.order.issue-part"].includes(command.operation)));
  const guardKey=`service-owner:${randomUUID()}`;
  await current(database,actor,guardKey);
  const replay=Boolean(await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,command.target.id,actor.userId,actor.companyId).first());
  if(!replay&&needsReview(command.operation))await current(database,actor,guardKey,true);
  const before=!command.creating||replay?await requireSource(database,actor,command.target):null;
  const dependencies=await Promise.all((command.sources??[]).map(async target=>({target,before:await requireSource(database,actor,target)})));
  const asset=Boolean(command.asset||before?.asset||dependencies.some(value=>value.before.asset));
  const scopes=serviceCommandPurchases(command.operation,asset).map(value=>({...value,companyId:actor.companyId,purpose:replay?"history" as const:value.purpose}));
  // Keep the original command error contract: absent history fails directly;
  // new-work/settlement changes fail the ordinary protected command guard.
  for(const value of scopes)await checkPurchasedCapability(database,mode,{...value,purpose:"history"});
  const boundaries=scopes.map((value,index)=>({key:`${guardKey}:${index}`,boundary:purchaseCommandBoundary(database,mode,`${guardKey}:${index}`,value)!}));
  await database.batch(boundaries.flatMap(value=>value.boundary.before));
  const composition:Composition={companyId:actor.companyId,assignmentId:actor.assignmentId,command,replay,checks:[]};compositions.set(database,composition);
  try{
   const result=await run(database),after=await requireSource(database,actor,command.target);
   if(before&&before.identity!==after.identity)throw new ApiError(409,"FIELD_SERVICE_SOURCE_CHANGED","The original Service source identity changed during this command.");
   for(const dependency of dependencies)if((await requireSource(database,actor,dependency.target)).image!==dependency.before.image)throw new ApiError(409,"FIELD_SERVICE_SOURCE_CHANGED","The captured agreement changed during this command.");
   await flushFinanceDeferredEffects(database);
   if((await requireSource(database,actor,command.target)).image!==after.image)throw new ApiError(409,"FIELD_SERVICE_SOURCE_CHANGED","The original Service source changed before commit.");
   for(const dependency of dependencies)if((await requireSource(database,actor,dependency.target)).image!==dependency.before.image)throw new ApiError(409,"FIELD_SERVICE_SOURCE_CHANGED","The captured Service agreement changed before commit.");
   await current(database,actor,guardKey,!replay&&needsReview(command.operation));
   for(const check of composition.checks)await check();
   const observed=await database.batch(boundaries.flatMap(value=>[...value.boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(value.key)]));boundaries[0]?.boundary.observe?.(observed);return result;
  }finally{compositions.delete(database);}
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}

export async function serviceFinalPurchaseCheck(database:SqlDatabase,actor:WorkContext,key:string,operation:string,usesAsset:boolean){
 const mode=await servicePackageEnforcementMode();if(mode==="disabled")return;
 const composition=compositions.get(database);
 if(!composition||composition.companyId!==actor.companyId||composition.assignmentId!==actor.assignmentId||composition.command.key!==key||composition.command.operation!==operation)throw new ApiError(503,"SERVICE_TRANSACTION_OWNERSHIP_REQUIRED","The Service command requires its registered owner transaction.");
 const scopes=serviceCommandPurchases(operation,usesAsset).map(scope=>({...scope,companyId:actor.companyId,purpose:composition.replay?"history" as const:scope.purpose}));
 for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);
 composition.checks.push(async()=>{for(const [index,scope] of scopes.entries()){
  const guard=`${key}:service-final:${index}`,boundary=purchaseCommandBoundary(database,mode,guard,scope)!;
  const observed=await database.batch([...boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(guard)]);boundary.observe?.(observed);
 }});
}
