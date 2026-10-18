import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "../../identity_access/public/authorization";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import {iamMutationGuards} from "../../identity_access/public/iam-store";
import {assertion,workContextGuards} from "../../workflow/public/commands";
import {manufacturingPackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults,companyPurchaseReadLocks} from "../../commercial/public/purchase-command-boundary";
import {financeDeferredEffectStatements} from "../../finance/public/finance-transactions";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {CapabilityPurpose} from "../../commercial/public/rules/packages";
import type {AttachmentOperation} from "../../files/public/attachments/types";
import {ApiError} from "../../runtime/public/http";
import {manufacturingPurchaseRecordRows,manufacturingPurchaseSourceReadLocks,type ManufacturingPurchaseTarget} from "./purchases";
export type ManufacturingAttachmentPurchase={companyId:string;capability:string;purpose:CapabilityPurpose;actor:WorkContext;targetId:string;kind:ManufacturingPurchaseTarget["kind"];advanced:boolean;witness:string};
function kind(type:string){return type==="manufacturing_spec"?"specification":type==="production_run"?"order":type==="planning_scenario"?"scenario":null;}
export async function manufacturingAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,type:string,id:string,purpose:CapabilityPurpose="settle-existing"):Promise<ManufacturingAttachmentPurchase|null>{
 const target=kind(type);if(!target||await manufacturingPackageEnforcementMode()==="disabled")return null;
 const row=await database.prepare(`SELECT capability,advanced,witness::text witness FROM (${manufacturingPurchaseRecordRows}) actual_production_file WHERE id=? AND company_id=? AND kind=?`).bind(id,actor.companyId,target).first<{capability:string;advanced:boolean;witness:string}>();if(!row)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","The original production source is unavailable.");return {companyId:actor.companyId,capability:row.capability,purpose,actor,targetId:id,kind:target,advanced:row.advanced,witness:row.witness};
}
function sourcePredicate(permission:string){return `EXISTS(SELECT 1 FROM (SELECT source.*,?::text department_id FROM (${manufacturingPurchaseRecordRows}) source) production_file_scope WHERE id=? AND company_id=? AND kind=? AND witness::text=? AND (branch_id IS NULL OR ?::text IS NULL OR branch_id=?) AND ${assignmentReadSql("production_file_scope",permission)} AND ?::jsonb @> jsonb_build_array(production_file_scope.module_id) AND EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array(production_file_scope.module_id))))`;}
function sourceArgs(scope:ManufacturingAttachmentPurchase){const actor=scope.actor;return [actor.departmentId,scope.targetId,scope.companyId,scope.kind,scope.witness,actor.branchId,actor.branchId,actor.assignmentId,actor.userId,actor.role,JSON.stringify(actor.modules),actor.assignmentId];}
function capabilities(scope:ManufacturingAttachmentPurchase){return [scope.capability,...(scope.advanced?["planning.advanced-mrp"]:[])];}
export async function authorizeManufacturingAttachmentPurchase(database:SqlDatabase,actor:WorkContext,type:string,id:string,operation:AttachmentOperation){
 const scope=await manufacturingAttachmentPurchaseScope(database,actor,type,id,operation==="read"?"history":"settle-existing");if(!scope)return;
 const current=await database.prepare(`SELECT ${sourcePredicate(operation==="read"?"records.read":"records.write")} allowed`).bind(...sourceArgs(scope)).first<{allowed:boolean}>();if(!current?.allowed)throw new ApiError(403,"MANUFACTURING_ATTACHMENT_CHANGED","The original production file assignment is unavailable.");
 for(const capability of capabilities(scope))await checkPurchasedCapability(database,await manufacturingPackageEnforcementMode(),{companyId:actor.companyId,capability,purpose:scope.purpose});
}
export async function manufacturingAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly ManufacturingAttachmentPurchase[]){if(!scopes.length||await manufacturingPackageEnforcementMode()==="disabled")return [];const locks=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(id=>companyPurchaseReadLocks(database,id));return [...locks.map(items=>items[0]),...manufacturingPurchaseSourceReadLocks(database,true),...locks.flatMap(items=>items.slice(1))];}
export async function manufacturingAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly ManufacturingAttachmentPurchase[]){
 const mode=await manufacturingPackageEnforcementMode();if(mode==="disabled"||!scopes.length)return;const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.kind}:${scope.targetId}:${scope.purpose}`,scope])).values()],boundaries=unique.flatMap(scope=>capabilities(scope).map(capability=>{const key=`manufacturing-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,{...scope,capability})!};}));
 const guards=(phase:string)=>unique.flatMap(scope=>{const actor=scope.actor,key=`manufacturing-file-${phase}:${randomUUID()}`;return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,"manufacturing-file-source",`${sourcePredicate("records.read")} AND ${sourcePredicate("records.write")}`,[...sourceArgs(scope),...sourceArgs(scope)]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];});
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function manufacturingAttachmentFailure(error:unknown){let current=error;for(let depth=0;depth<5&&current&&typeof current==="object";depth++){const item=current as {code?:string;detail?:string;cause?:unknown};if(item.code==="23514"&&item.detail?.includes("manufacturing-file-"))return new ApiError(409,"MANUFACTURING_ATTACHMENT_CHANGED","The original production source or current file authority changed.");current=item.cause;}return null;}
