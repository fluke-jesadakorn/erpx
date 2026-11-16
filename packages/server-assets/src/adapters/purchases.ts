import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards,financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {flushFinanceDeferredEffects} from "@erpx/server-finance/public/finance-transactions";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {isTransactionalSqlDatabase,type SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assetsPackageEnforcementMode,checkPurchasedCapability,readPurchasedAccesses,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";

import {maintenancePurchaseSourceQuery,maintenancePurchaseSourceReadLocks} from "@erpx/server-maintenance/public/purchase-sources";

type Capability="assets.register"|"assets.custody"|"assets.maintenance"|"assets.downtime"|"assets.preventive"|"assets.parts-costs";
export type Target={kind:"asset"|"work"|"plan";id:string};
type Command={key:string;operation:string;capability:Capability;purpose:CapabilityPurpose;target:Target;creating?:boolean;permissions:readonly string[]};
type Composition={companyId:string;assignmentId:string;checks:(()=>Promise<void>)[]};
const compositions=new WeakMap<SqlDatabase,Composition>();
const scope="r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)";
function sourceQuery(target:Target,file=false){
 if(target.kind==="asset")return `SELECT jsonb_build_object('asset',${file?"to_jsonb(r)-'version'-'updated_at'":"to_jsonb(r)"},'finance',to_jsonb(f),'links',(SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM operational_asset_finance_links link WHERE link.asset_id=r.id))::text image FROM operational_assets r LEFT JOIN finance_assets f ON f.id=r.finance_asset_id AND f.company_id=r.company_id WHERE r.id=? AND ${scope}`;
 return maintenancePurchaseSourceQuery(target.kind,file);
}
const sourceArgs=(actor:WorkContext,target:Target)=>[target.id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId];
export async function assetOwnerSourceImage(database:SqlDatabase,actor:WorkContext,target:Target,file=false){return database.prepare(sourceQuery(target,file)).bind(...sourceArgs(actor,target)).first<string>("image");}
const source=assetOwnerSourceImage;
export function assetOwnerSourceGuards(database:SqlDatabase,actor:WorkContext,target:Target,key:string,witness:string,file=false){return [assertion(database,key,file?"asset-file-owner":"assets-owner-source",`(${sourceQuery(target,file)})=?`,[...sourceArgs(actor,target),witness]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];}
/** Writer leases serialize before the source snapshot; SHARE followed by the
 * original Asset update can deadlock between unrelated Companies. */
export function assetOwnerSourceReadLocks(database:SqlDatabase,companyId:string){const locks=companyPurchaseReadLocks(database,companyId);return [locks[0],database.prepare("LOCK TABLE operational_assets,operational_asset_finance_links IN SHARE ROW EXCLUSIVE MODE"),database.prepare("LOCK TABLE finance_assets IN SHARE MODE"),...maintenancePurchaseSourceReadLocks(database),...locks.slice(1)];}
export async function assetCurrentAuthority(database:SqlDatabase,actor:WorkContext,permissions:readonly string[],key:string){
 for(const permission of permissions){const allowed=await database.prepare(`SELECT ${assignmentReadSql("assets_scope",permission)} allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) assets_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first<{allowed:boolean}>();if(!allowed?.allowed)throw new ApiError(403,"ASSETS_ACCESS_CHANGED","Choose a current authorized Assets or Maintenance work role.");}
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}
const authority=assetCurrentAuthority;
export async function assetPurchaseControls(actor:WorkContext,capabilities:readonly Capability[]=["assets.register","assets.custody","assets.maintenance","assets.downtime","assets.preventive","assets.parts-costs"]){
 const mode=await assetsPackageEnforcementMode();if(mode==="disabled")return undefined;const database=(await holdingResources(actor.holdingId)).write;await authority(database,actor,["records.read"],`asset-controls:${randomUUID()}`);const access=await readPurchasedAccesses(database,mode,actor.companyId,capabilities);
 return Object.fromEntries(Object.entries(access).map(([capability,purchase])=>[capability,{newWork:purchase.newWork,settleExisting:purchase.settleExisting,history:purchase.history,revision:purchase.revision}]));
}
export async function withAssetPurchasedRead<Result>(actor:WorkContext,capability:Capability,run:(database?:SqlDatabase)=>Promise<Result>,provided?:SqlDatabase){
 const mode=await assetsPackageEnforcementMode();if(mode==="disabled")return run(provided);const database=provided??(await holdingResources(actor.holdingId)).write;await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability,purpose:"history"});await authority(database,actor,["records.read"],`asset-read:${randomUUID()}`);return run(database);
}
/** Preventive generation composes its real Work and physical Asset changes in
 * this private owned unit. Unknown caller transactions cannot bypass a fence. */
export async function withAssetPurchasedCommand<Result>(actor:WorkContext,command:Command,run:(database?:SqlDatabase)=>Promise<Result>,provided?:SqlDatabase){
 const mode=await assetsPackageEnforcementMode(),owner=(await holdingResources(actor.holdingId)).write;if(mode==="disabled")return run(provided);
 const execute=async(database:SqlDatabase)=>{
  const composition=compositions.get(database);if(!composition||composition.companyId!==actor.companyId||composition.assignmentId!==actor.assignmentId)throw new ApiError(503,"ASSETS_TRANSACTION_OWNERSHIP_REQUIRED","Use the original owned Assets transaction.");
  const receipt=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,actor.userId,actor.companyId).first(),purchase={companyId:actor.companyId,capability:command.capability,purpose:receipt?"history" as const:command.purpose},key=`asset-purchase:${randomUUID()}`;
  await checkPurchasedCapability(database,mode,purchase);await authority(database,actor,command.permissions,key);
  if(!command.creating&&!await source(database,actor,command.target))throw new ApiError(404,"ASSETS_SOURCE_NOT_FOUND","The actual Assets or Maintenance source was not found in this workspace.");
  const boundary=purchaseCommandBoundary(database,mode,key,purchase)!;await database.batch(boundary.before);const result=await run(database),witness=await source(database,actor,command.target);if(!witness)throw new ApiError(409,"ASSETS_SOURCE_CHANGED","The actual owner source changed.");
  composition.checks.push(async()=>{await authority(database,actor,command.permissions,key);const observed=await database.batch([assertion(database,key,"assets-owner-source",`(${sourceQuery(command.target)})=?`,[...sourceArgs(actor,command.target),witness]),...boundary.after,database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);boundary.observe?.(observed);});return result;
 };
 try{
  if(provided)return await execute(provided);
  if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"TRANSACTION_REQUIRED","Assets and Maintenance require their original Holding transaction.");
  return await owner.transaction(async database=>{
   await database.batch(assetOwnerSourceReadLocks(database,actor.companyId));
   const composition:Composition={companyId:actor.companyId,assignmentId:actor.assignmentId,checks:[]};compositions.set(database,composition);
   try{const result=await execute(database);await flushFinanceDeferredEffects(database);for(const check of composition.checks)await check();return result;}finally{compositions.delete(database);}
  });
 }catch(error){if((error as {code?:string}).code==="23514"&&String((error as {detail?:string}).detail).includes("assets-owner-source")){const failure=new ApiError(409,"ASSETS_SOURCE_CHANGED","The Assets or Maintenance source changed before commit. Refresh before retrying.");failure.cause=error;throw failure;}throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
