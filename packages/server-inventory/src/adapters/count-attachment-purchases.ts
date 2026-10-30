import {serviceCountPersistedMarker,serviceInitialPhysicalStockCountSource,serviceInitialPhysicalStockCapabilities,serviceInitialPhysicalStockCountSourceRows,serviceInitialPhysicalStockSourceLeases} from '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source';
import {posInitialPhysicalStockCountMarked,posInitialPhysicalStockCountSource,posInitialPhysicalStockCapabilities,posInitialPhysicalStockCountSourceRows,posInitialPhysicalStockSourceLeases} from '@erpx/server-inventory/adapters/pos-initial-physical-stock-count-source';
import {manufacturingInitialPhysicalStockCountMarked,manufacturingInitialPhysicalStockCountSource,manufacturingInitialPhysicalStockCapabilities,manufacturingInitialPhysicalStockCountSourceRows,manufacturingInitialPhysicalStockSourceLeases} from '@erpx/server-inventory/adapters/manufacturing-initial-physical-stock-count-source';
import {initialPhysicalStockCountMarked,initialPhysicalStockCountSource,initialPhysicalStockCapabilities,initialPhysicalStockCountSourceRows} from '@erpx/server-inventory/adapters/initial-physical-stock-count-source';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';
import {commercePackageEnforcementMode,purchaseCommandBoundary,observePurchaseResults,checkPurchasedCapability,readPurchasedAccesses,companyPurchaseReadLocks} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {AttachmentOperation} from "@erpx/server-files/public/attachments/types";
import {financeDeferredEffectStatements} from "@erpx/server-finance/public/finance-transactions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {qualityDisposalCountRows} from "@erpx/server-inventory/adapters/quality-count-purchases";
import {qualityPurchaseSourceReadLocks} from "@erpx/server-quality/public/purchase-sources";
import {requireCountPurchasedRead} from "@erpx/server-inventory/adapters/count-purchases";

export type CountAttachmentPurchase={companyId:string;capability:string;additionalCapabilities:readonly string[];qualityWitness:string|null;initialWitness:string|null;manufacturingWitness:string|null;posWitness:string|null;serviceWitness:string|null;purpose:CapabilityPurpose;actor:WorkContext;targetId:string;witness:string};
const target=(type:string)=>type==="stock_count"||type==="inventory_stock_count";
const image="SELECT jsonb_build_object('count',to_jsonb(count_file_source)-'version'-'updated_at','lines',COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM inventory_count_lines l WHERE l.count_id=count_file_source.id),'[]'::jsonb),'lots',COALESCE((SELECT jsonb_agg(to_jsonb(lot) ORDER BY lot.id) FROM inventory_count_lot_lines lot JOIN inventory_count_lines l ON l.id=lot.count_line_id WHERE l.count_id=count_file_source.id),'[]'::jsonb))::text image FROM inventory_counts count_file_source WHERE id=? AND company_id=?";
const disposalImage=`SELECT to_jsonb(disposal)::text image FROM (${qualityDisposalCountRows}) disposal WHERE id=? AND company_id=? AND kind='customer-return'`;
export async function countAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,type:string,id:string,purpose:CapabilityPurpose="settle-existing"):Promise<CountAttachmentPurchase|null>{
 if(!target(type))return null;
 const service=await serviceCountPersistedMarker(database,id),pos=await posInitialPhysicalStockCountMarked(database,id),manufacturing=await manufacturingInitialPhysicalStockCountMarked(database,id),initial=await initialPhysicalStockCountMarked(database,id);if(!initial&&!manufacturing&&!pos&&!service&&await commercePackageEnforcementMode()==='disabled')return null;
 const witness=await database.prepare(image).bind(id,actor.companyId).first<string>("image");
 if(!witness)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","The original physical count source is unavailable.");
 if(service){const native=await database.prepare('SELECT 1 FROM inventory_service_initial_physical_sources WHERE count_id=? AND company_id=?').bind(id,actor.companyId).first();const serviceWitness=await serviceInitialPhysicalStockCountSource(database,actor,id);if(!native||!serviceWitness)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original Service physical Count source is unavailable.');return {companyId:actor.companyId,capability:serviceInitialPhysicalStockCapabilities[0],additionalCapabilities:serviceInitialPhysicalStockCapabilities.slice(1),qualityWitness:null,initialWitness:null,manufacturingWitness:null,serviceWitness,posWitness:null,purpose,actor,targetId:id,witness};}
 if(pos){const posWitness=await posInitialPhysicalStockCountSource(database,actor,id);if(!posWitness)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original POS physical Count source is unavailable.');return {companyId:actor.companyId,capability:posInitialPhysicalStockCapabilities[0],additionalCapabilities:posInitialPhysicalStockCapabilities.slice(1),qualityWitness:null,initialWitness:null,manufacturingWitness:null,posWitness,serviceWitness:null,purpose,actor,targetId:id,witness};}
 if(manufacturing){const manufacturingWitness=await manufacturingInitialPhysicalStockCountSource(database,actor,id);if(!manufacturingWitness)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original Manufacturing physical Count source is unavailable.');return {companyId:actor.companyId,capability:manufacturingInitialPhysicalStockCapabilities[0],additionalCapabilities:manufacturingInitialPhysicalStockCapabilities.slice(1),qualityWitness:null,initialWitness:null,manufacturingWitness,posWitness:null,serviceWitness:null,purpose,actor,targetId:id,witness};}
 if(initial){const initialWitness=await initialPhysicalStockCountSource(database,actor,id);if(!initialWitness)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original Initial Physical Stock Count source is unavailable.');return {companyId:actor.companyId,capability:initialPhysicalStockCapabilities[0],additionalCapabilities:initialPhysicalStockCapabilities.slice(1),qualityWitness:null,initialWitness,manufacturingWitness:null,posWitness:null,serviceWitness:null,purpose,actor,targetId:id,witness};}
 const base=await readPurchasedAccesses(database,await commercePackageEnforcementMode(),actor.companyId,["supply.counts"]),qualityWitness=base["supply.counts"].codes[purpose]!=="GRANTED"?await database.prepare(disposalImage).bind(id,actor.companyId).first<string>("image"):null;
 return {companyId:actor.companyId,capability:qualityWitness?"sales.orders":"supply.counts",additionalCapabilities:qualityWitness?["inventory.stock-core"]:[],qualityWitness,initialWitness:null,manufacturingWitness:null,posWitness:null,serviceWitness:null,purpose,actor,targetId:id,witness};
}
/** The original Files policy authorizes first, including review phase and Legal
 * holds. This owner gate covers purchase/current assignment and actual source. */
export async function authorizeCountAttachmentPurchase(database:SqlDatabase,actor:WorkContext,type:string,id:string,operation:AttachmentOperation){
 if(!target(type)||await commercePackageEnforcementMode()==='disabled'&&!await initialPhysicalStockCountMarked(database,id)&&!await manufacturingInitialPhysicalStockCountMarked(database,id)&&!await posInitialPhysicalStockCountMarked(database,id)&&!await serviceCountPersistedMarker(database,id))return;
 if(await serviceCountPersistedMarker(database,id))await countAttachmentPurchaseScope(database,actor,type,id,operation==='read'?'history':'settle-existing');
 await requireCountPurchasedRead(database,actor,id);
 const scope=await countAttachmentPurchaseScope(database,actor,type,id,operation==="read"?"history":"settle-existing");if(scope)for(const capability of [scope.capability,...scope.additionalCapabilities])await checkPurchasedCapability(database,scope.initialWitness||scope.manufacturingWitness||scope.posWitness||scope.serviceWitness?'enforced':await commercePackageEnforcementMode(),{companyId:actor.companyId,capability,purpose:scope.purpose});
}
export async function countAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly CountAttachmentPurchase[]){
 if(!scopes.length||await commercePackageEnforcementMode()==="disabled"&&!scopes.some(scope=>scope.initialWitness||scope.manufacturingWitness||scope.posWitness||scope.serviceWitness))return [];
 const locks=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(id=>companyPurchaseReadLocks(database,id));
 return [...(scopes.some(scope=>scope.serviceWitness)?[database.prepare('LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE'),...serviceInitialPhysicalStockSourceLeases(database)]:[]),...(scopes.some(scope=>scope.posWitness)?[database.prepare('LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE'),...posInitialPhysicalStockSourceLeases(database)]:[]),...(scopes.some(scope=>scope.manufacturingWitness)?[database.prepare('LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE'),...manufacturingInitialPhysicalStockSourceLeases(database)]:[]),...locks.map(items=>items[0]),...qualityPurchaseSourceReadLocks(database),...locks.flatMap(items=>items.slice(1))];
}
export async function countAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly CountAttachmentPurchase[]){
 const mode=await commercePackageEnforcementMode();if(!scopes.length||mode==="disabled"&&!scopes.some(scope=>scope.initialWitness||scope.manufacturingWitness||scope.posWitness||scope.serviceWitness))return undefined;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.targetId}:${scope.purpose}`,scope])).values()];
 const boundaries=unique.flatMap(scope=>[scope.capability,...scope.additionalCapabilities].map(capability=>{const key=`count-file:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,(scope.initialWitness||scope.manufacturingWitness||scope.posWitness||scope.serviceWitness)?"enforced":mode,key,{...scope,capability})!};}));
 const guards=(phase:string)=>unique.flatMap(scope=>{
  const key=`count-file-${phase}:${randomUUID()}`,actor=scope.actor;
  return [database.prepare("SELECT id FROM inventory_counts WHERE id=? AND company_id=? FOR SHARE").bind(scope.targetId,scope.companyId),database.prepare("SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE").bind(actor.assignmentId,actor.companyId),...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),
   ...(scope.initialWitness?[assertion(database,key,'count-file-initial-source',`EXISTS(SELECT 1 FROM (${initialPhysicalStockCountSourceRows}) initial_source WHERE initial_source.id=? AND initial_source.company_id=? AND initial_source.source_proof::text=? AND ${['sales','inventory','finance'].map(module=>scheduledFeaturePredicate('initial_source.company_id',module)).join(' AND ')})`,[scope.targetId,scope.companyId,scope.initialWitness])]:[]),
   ...(scope.serviceWitness?[assertion(database,key,'count-file-service-source',`EXISTS(SELECT 1 FROM (${serviceInitialPhysicalStockCountSourceRows}) service_source WHERE service_source.id=? AND service_source.company_id=? AND service_source.source_proof::text=? AND ${['field_service','inventory','finance'].map(module=>scheduledFeaturePredicate('service_source.company_id',module)).join(' AND ')})`,[scope.targetId,scope.companyId,scope.serviceWitness])]:[]),
   ...(scope.posWitness?[assertion(database,key,'count-file-pos-source',`EXISTS(SELECT 1 FROM (${posInitialPhysicalStockCountSourceRows}) pos_source WHERE pos_source.id=? AND pos_source.company_id=? AND pos_source.source_proof::text=? AND ${['pos','inventory','finance'].map(module=>scheduledFeaturePredicate('pos_source.company_id',module)).join(' AND ')})`,[scope.targetId,scope.companyId,scope.posWitness])]:[]),
   ...(scope.manufacturingWitness?[assertion(database,key,'count-file-manufacturing-source',`EXISTS(SELECT 1 FROM (${manufacturingInitialPhysicalStockCountSourceRows}) manufacturing_source WHERE manufacturing_source.id=? AND manufacturing_source.company_id=? AND manufacturing_source.source_proof::text=? AND ${['manufacturing','inventory','finance'].map(module=>scheduledFeaturePredicate('manufacturing_source.company_id',module)).join(' AND ')})`,[scope.targetId,scope.companyId,scope.manufacturingWitness])]:[]),
   ...(scope.qualityWitness?[assertion(database,key,"count-file-quality-source",`(${disposalImage})=?`,[scope.targetId,scope.companyId,scope.qualityWitness])]:[]),
   assertion(database,key,"count-file-owner",`(${image})=?`,[scope.targetId,scope.companyId,scope.witness]),
   assertion(database,key,"count-file-authority",`EXISTS(SELECT 1 FROM (SELECT id,company_id,branch_id,NULL::text department_id FROM inventory_counts) count_file_scope WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND ?::text IS NULL AND (${assignmentReadSql("count_file_scope","inventory.prepare")} OR ${assignmentReadSql("count_file_scope","inventory.approve")}))`,[scope.targetId,scope.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role]),
   database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];
 });
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function countAttachmentFailure(error:unknown){
 let current=error;
 for(let depth=0;depth<5&&current&&typeof current==="object";depth++){
  const failure=current as {code?:string;detail?:string;cause?:unknown};
  if(failure.code==="23514"&&failure.detail?.includes("count-file-"))return new ApiError(409,"COUNT_ATTACHMENT_CHANGED","The physical count source or current file authority changed. Refresh before retrying.");
  current=failure.cause;
 }
 return null;
}
