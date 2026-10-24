import "server-only";
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
import {inventoryTransferSources} from "@erpx/server-commerce/public/inventory-transfers-sources";
import {inventoryTransitSources} from "@erpx/server-commerce/public/inventory-transit-sources";

type Actor=Pick<WorkContext,"userId"|"assignmentId"|"role"|"companyId"|"branchId"|"departmentId"|"authority"|"iam">;
export type TransferPurchaseTarget={kind:"transfer"|"transit";id:string};
type Source={id:string;company_id:string;source_branch_id:string;target_branch_id:string;source_warehouse_id:string;target_warehouse_id:string;transfer_id?:string;chart_id?:string;inventory_account_id?:string;currency?:string;source_warehouse_version?:number;target_warehouse_version?:number};
async function source(database:SqlDatabase,actor:Actor,target:TransferPurchaseTarget){
 const query=target.kind==="transfer"?`SELECT id,company_id,source_branch_id,target_branch_id,source_warehouse_id,target_warehouse_id,source_warehouse_version,target_warehouse_version FROM inventory_transfers
  WHERE id=? AND company_id=? AND (?::text IS NULL OR source_branch_id=? OR target_branch_id=?)`:
 `SELECT d.id,d.company_id,d.source_branch_id,d.target_branch_id,d.source_warehouse_id,d.target_warehouse_id,d.transfer_id,d.chart_id,d.inventory_account_id,d.currency
  FROM inventory_transit d JOIN inventory_transfers t ON t.id=d.transfer_id AND t.company_id=d.company_id
  AND t.source_warehouse_id=d.source_warehouse_id AND t.target_warehouse_id=d.target_warehouse_id
  AND t.source_branch_id=d.source_branch_id AND t.target_branch_id=d.target_branch_id AND t.delivery_mode='transit'
  WHERE d.id=? AND d.company_id=? AND (?::text IS NULL OR d.source_branch_id=? OR d.target_branch_id=?)`;
 const row=await database.prepare(query).bind(target.id,actor.companyId,actor.branchId,actor.branchId,actor.branchId).first<Source>();
 if(!row)throw new ApiError(404,target.kind==="transfer"?"TRANSFER_NOT_FOUND":"TRANSIT_NOT_FOUND","This warehouse movement is unavailable in the current workspace.");
 return row;
}
async function current(database:SqlDatabase,actor:Actor,key:string,permissions:readonly string[]){
 if(actor.departmentId)throw new ApiError(403,"TRANSFER_ACCESS_CHANGED","Choose a current inventory assignment.");
 for(const permission of new Set(["inventory.read",...permissions])){
  if(!hasPermission(actor,permission))throw new ApiError(403,"TRANSFER_ACCESS_CHANGED","Choose a current authorized warehouse assignment.");
  const assignment=await database.prepare(`SELECT ${assignmentReadSql("transfer_scope",permission)} allowed FROM (SELECT ?::text company_id,?::text branch_id,NULL::text department_id) transfer_scope`)
   .bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId).first<{allowed:boolean}>();
  if(!assignment?.allowed)throw new ApiError(403,"TRANSFER_ACCESS_CHANGED","The original warehouse assignment is no longer current.");
 }
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}
/** Supply management only. Shared stock facts used by POS, Hotel, Sales and
 * Manufacturing retain their named owner contracts. */
export async function transferSourceAccess(database:SqlDatabase,actor:Actor){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return undefined;
 await current(database,actor,`transfer-read:${randomUUID()}`,[]);
 const access=await readPurchasedAccesses(database,mode,actor.companyId,["supply.transfers","finance.inventory-valuation"]),owner=access["supply.transfers"];
 return {newWork:owner.newWork,settleExisting:owner.settleExisting,history:owner.history,revision:owner.revision,lossSettleExisting:owner.settleExisting&&access["finance.inventory-valuation"].settleExisting};
}
export async function transferPurchaseControls(actor:WorkContext){return transferSourceAccess((await holdingResources(actor.holdingId)).write,actor);}
export async function requireTransferPurchasedRead(database:SqlDatabase,actor:Actor,target?:TransferPurchaseTarget){
 const access=await transferSourceAccess(database,actor);
 if(access&&!access.history)throw new ApiError(403,"PACKAGE_REQUIRED","Warehouse transfers require the Supply purchase or retained history.");
 if(access&&target)await source(database,actor,target);
 return access;
}
export async function transferDocumentFlow(database:SqlDatabase,actor:Actor,view:FlowView){
 const access=await requireTransferPurchasedRead(database,actor,{kind:"transfer",id:view.recordId});
 const canAct=!access||(["draft","rejected"].includes(view.state)?access.newWork:access.settleExisting);
 return canAct?view:{...view,canAct:false,canManage:false};
}
/** The original full owner reads, stock/evidence/journal effects and receipt
 * commit run inside one Holding transaction; final deferred effects are fenced. */
export async function withTransferPurchasedCommand<Result>(owner:SqlDatabase,actor:Actor,command:{key:string;operation:string;target:TransferPurchaseTarget;purpose:CapabilityPurpose;permissions:readonly string[];existing?:boolean;financial?:boolean;flowStep?:string;saveDefault?:boolean;warehousePolicy?:"both"|"source"|"target";refreshRoute?:boolean},run:(database:SqlDatabase)=>Promise<Result>){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return run(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"TRANSFER_TRANSACTION_REQUIRED","Warehouse movement requires the original owned Holding transaction.");
 try{return await owner.transaction(async database=>{
  await database.batch(companyPurchaseReadLocks(database,actor.companyId));
  const replay=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?")
   .bind(command.key,command.operation,command.target.id,actor.userId,actor.companyId).first();
  const before=command.existing||replay?await source(database,actor,command.target):null;
  const flow=command.flowStep?await database.prepare("SELECT status FROM inventory_transfers WHERE id=? AND company_id=?").bind(command.target.id,actor.companyId).first<{status:string}>():null;
  const flowNew=command.flowStep&&(command.saveDefault||["draft","rejected"].includes(flow?.status??"")||command.flowStep==="draft");
  const purpose=replay?"history" as const:flowNew?"new-work" as const:command.purpose;
  const scopes=[{companyId:actor.companyId,capability:"supply.transfers",purpose},...(command.financial?[{companyId:actor.companyId,capability:"finance.inventory-valuation",purpose}]:[])];
  const key=`transfer-purchase:${randomUUID()}`,boundaries=scopes.map((scope,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,mode,`${key}:${index}`,scope)!}));
  for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);
  await current(database,actor,key,command.permissions);
  await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const result=await run(database);
  await flushFinanceDeferredEffects(database);
  const after=await source(database,actor,command.target);
  if(before&&Object.keys(before).filter(field=>!field.endsWith("_warehouse_version") && !(command.refreshRoute && ["source_branch_id","target_branch_id"].includes(field))).some(field=>before[field as keyof Source]!==after[field as keyof Source]))throw new ApiError(409,"TRANSFER_SOURCE_CHANGED","The original source, target or custody ownership changed.");
  await current(database,actor,key,command.permissions);
  const locations=command.warehousePolicy==="both"?["source","target"] as const:command.warehousePolicy?[command.warehousePolicy]:[];
  const warehouseGuards=replay?[]:locations.map(side=>command.target.kind==="transfer"
   ?inventoryTransferSources(database).guardWarehouseVersion(key,`final-${side}-warehouse`,after[`${side}_warehouse_id` as keyof Source]??null,actor.companyId,after[`${side}_branch_id` as keyof Source]??null,after[`${side}_warehouse_version` as keyof Source]??null)
   :inventoryTransitSources(database).guardActiveLocation(key,`final-${side}-warehouse`,after[`${side}_warehouse_id` as keyof Source]??null,actor.companyId,after[`${side}_branch_id` as keyof Source]??null));
  const observations=await database.batch([...warehouseGuards,...boundaries.flatMap(item=>[...item.boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key)]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
  boundaries[0]?.boundary.observe?.(observations);return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
