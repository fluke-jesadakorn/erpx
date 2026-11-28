import "server-only";
import {assetProjectionFromSql} from "./asset-scope-tuple";
import {hasModuleAccess} from "@erpx/platform-catalogue/module-access";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {assignmentReadSql} from "@erpx/identity-access-server/public/record-source-authority";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {assetsPackageEnforcementMode,checkPurchasedCapability,readPurchasedAccess} from "@erpx/commercial-server/public/purchase-command-boundary";
import {purchasedCapabilityPredicate} from "@erpx/commercial-server/public/purchased-capability-sql";
import {nativeAssetsClockScalar} from "./native-assets-read";

export type AssetSourceKind="asset"|"maintenance_work"|"maintenance_plan";
const definitions={asset:{table:"operational_assets",module:"assets",capability:"assets.register"},maintenance_work:{table:"maintenance_work_orders",module:"maintenance",capability:"assets.maintenance"},maintenance_plan:{table:"maintenance_preventive_plans",module:"maintenance",capability:"assets.preventive"}} as const;
/** Canonical owner-local fixed row-existence probe for Assets. Visibility,
 * record-source, clock-purchase and package-settlement EXISTS checks share this
 * one literal so the TS read tail carries no duplicate statement. The head is
 * the owner-local `assetProjectionFromSql` literal, from which this probe renders
 * byte-identically. Callers pass only owner-owned FROM and predicate fragments;
 * no SQL or relation name is accepted from a request. */
export const assetSourceProbeSql=(from:string,where:string)=>assetProjectionFromSql("1",` ${from}`,` WHERE ${where}`);
const assetSourceVisibleSql=(table:string,alias:string,idValue:string,tail="")=>`EXISTS(${assetSourceProbeSql(`${table} ${alias}`,`${alias}.id=${idValue}${tail}`)})`;
const assetSourceReadSql=(from:string,idColumn:string,tail="")=>assetSourceProbeSql(from,`${idColumn}=?${tail}`);
/** Fixed owner kinds select the actual source table. Project/Learning worker
 * references and generic records do not acquire an Assets purchase rule. */
export async function assetSourcePredicate(database:SqlDatabase,actor:WorkContext,alias:string,kind:AssetSourceKind,purchased=true){
 const mode=await assetsPackageEnforcementMode();if(mode==="disabled")return {where:"TRUE",args:[] as (string|number|null)[]};const definition=definitions[kind];
 if(!hasPermission(actor,"records.read")||!hasModuleAccess(actor.modules,definition.module))return {where:"FALSE",args:[] as (string|number|null)[]};
 if(mode==="shadow")await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:definition.capability});
 const purchase=mode==="enforced"&&purchased?`EXISTS(WITH asset_source_clock AS MATERIALIZED (${await nativeAssetsClockScalar()}) ${assetSourceProbeSql("asset_source_clock",purchasedCapabilityPredicate(`${alias}.company_id`,definition.capability,"history","asset_source_clock.now_ms"))})`:"TRUE";
 return {where:`${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?) AND ${assignmentReadSql(alias,"records.read")} AND ${purchase}`,args:[actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,actor.assignmentId,actor.userId,actor.role]};
}
export async function assetSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"records"|"staff"|"inbox"){
 const mode=await assetsPackageEnforcementMode();if(mode!=="enforced"){
  if(mode==="shadow")for(const capability of ["assets.register","assets.maintenance","assets.preventive"])await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability});
  return {where:"TRUE",args:[] as (string|number|null)[]};
 }
 const id=surface==="records"?"workspace_records.id":surface==="staff"?"staff_notifications.record_id":"e.source_id",company=surface==="records"?"workspace_records.company_id":surface==="staff"?"staff_notifications.company_id":"e.company_id",type=surface==="staff"?"staff_notifications.record_type":"e.source_type";
 const sources=await Promise.all((Object.entries(definitions) as [AssetSourceKind,(typeof definitions)[AssetSourceKind]][]).map(async([kind,definition])=>{
  const alias=`asset_native_${kind}`,scope=await assetSourcePredicate(database,actor,alias,kind),visible=assetSourceVisibleSql(definition.table,alias,id,` AND ${alias}.company_id=${company} AND ${scope.where}`);
  return {kind,definition,visible,args:scope.args};
 }));
 if(surface==="records")return {where:`(NOT(${sources.map(({definition})=>`(module_id='${definition.module}' AND ${assetSourceVisibleSql(definition.table,"actual_asset_source",id)})`).join(" OR ")}) OR ${sources.map(source=>source.visible).join(" OR ")})`,args:sources.flatMap(source=>source.args)};
 return {where:`(${type} NOT IN ('asset','maintenance_work','maintenance_plan') OR ${sources.map(source=>`(${type}='${source.kind}' AND ${source.visible})`).join(" OR ")})`,args:sources.flatMap(source=>source.args)};
}
export async function requireAssetWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 const mode=await assetsPackageEnforcementMode();if(mode==="disabled"||!["assets","maintenance"].includes(moduleId))return;
 for(const [kind,definition] of Object.entries(definitions) as [AssetSourceKind,(typeof definitions)[AssetSourceKind]][]){
  if(definition.module!==moduleId||!await database.prepare(assetSourceReadSql(definition.table,"id")).bind(id).first())continue;
  const scope=await assetSourcePredicate(database,actor,"asset_record_source",kind,false);
  if(!await database.prepare(assetSourceReadSql(`${definition.table} asset_record_source`,"id",` AND ${scope.where}`)).bind(id,...scope.args).first())throw new ApiError(404,"ASSETS_SOURCE_NOT_FOUND","The actual Assets or Maintenance source is unavailable in this workspace.");
  await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:definition.capability,purpose:"history"});
 }
}
