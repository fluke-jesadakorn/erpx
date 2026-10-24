import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';
import {commercePackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults,companyPurchaseReadLocks} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {financeDeferredEffectStatements} from "@erpx/server-finance/public/finance-transactions";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import type {AttachmentOperation,AttachmentPolicy} from "@erpx/server-files/public/attachments/types";
import {qualityPurchaseSourceRows,qualityPurchaseSourceReadLocks} from "@erpx/server-quality/adapters/purchase-sources";
import {requireQualityPurchasedRead} from "@erpx/server-quality/adapters/purchases";
export type QualityAttachmentPurchase={companyId:string;capabilities:readonly string[];purpose:CapabilityPurpose;actor:WorkContext;kind:"hold"|"spec";targetId:string;witness:string;permission:string};
const kind=(type:string)=>["quality_hold","inventory_quality_hold"].includes(type)?"hold":type==="inventory_quality_spec"?"spec":null;
function source(type:"hold"|"spec"){
 return type==="hold"?`SELECT id,company_id,branch_id,NULL::text department_id,kind,witness::text image FROM (${qualityPurchaseSourceRows}) actual_quality_source WHERE id=? AND company_id=?`:"SELECT id,company_id,NULL::text branch_id,NULL::text department_id,'advanced' kind,(to_jsonb(s)-'version'-'updated_at')::text image FROM inventory_quality_specs s WHERE id=? AND company_id=?";
}
export async function qualityAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,"targetType"|"purposeKey">,targetId:string):Promise<QualityAttachmentPurchase|null>{
 const target=kind(policy.targetType);if(!target||await commercePackageEnforcementMode()==="disabled")return null;
 const row=await database.prepare(source(target)).bind(targetId,actor.companyId).first<{image:string;kind:string}>();if(!row)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","The original Quality file source is unavailable.");
 const permission=target==="spec"?"quality.release":["quality_inspection_evidence","inspection_evidence"].includes(policy.purposeKey)?"quality.inspect":policy.purposeKey==="disposition_evidence"?"quality.release":"quality.hold";
 return {companyId:actor.companyId,capabilities:row.kind==="receipt"?["supply.receiving","supply.stock"]:row.kind==="customer-return"?["sales.orders","inventory.stock-core"]:["supply.quality"],purpose:target==="spec"?"new-work":"settle-existing",actor,kind:target,targetId,witness:row.image,permission};
}
export async function authorizeQualityAttachmentPurchase(database:SqlDatabase,actor:WorkContext,policy:AttachmentPolicy,id:string,operation:AttachmentOperation){
 const scope=await qualityAttachmentPurchaseScope(database,actor,policy,id);if(!scope)return;
 await requireQualityPurchasedRead(database,actor,scope.kind==="hold"?id:undefined);
 for(const capability of scope.capabilities)await checkPurchasedCapability(database,await commercePackageEnforcementMode(),{companyId:actor.companyId,capability,purpose:operation==="read"?"history":scope.purpose});
}
/** Acquire relation leases before the first Files source snapshot, including
 * absent origin/allocation/disposition relations. No shared Stock gate. */
export async function qualityAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly QualityAttachmentPurchase[]){
 if(!scopes.length||await commercePackageEnforcementMode()==="disabled")return [];
 const companies=[...new Set(scopes.map(scope=>scope.companyId))].sort(),locks=companies.map(id=>companyPurchaseReadLocks(database,id));
 return [...locks.map(items=>items[0]),...qualityPurchaseSourceReadLocks(database),database.prepare("LOCK TABLE inventory_quality_specs,inventory_quality_spec_metrics IN SHARE MODE"),...locks.flatMap(items=>items.slice(1))];
}
export async function qualityAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly QualityAttachmentPurchase[]){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled"||!scopes.length)return;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.kind}:${scope.targetId}:${scope.permission}:${scope.purpose}`,scope])).values()];
 const boundaries=unique.flatMap(scope=>scope.capabilities.map(capability=>{const key=`quality-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,{companyId:scope.companyId,capability,purpose:scope.purpose})!};}));
 const guards=(phase:string)=>unique.flatMap(scope=>{const actor=scope.actor,key=`quality-file-${phase}:${randomUUID()}`;
 return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,"quality-file-source",`EXISTS(SELECT 1 FROM (${source(scope.kind)}) quality_file_source WHERE image=? AND (?::text IS NULL OR branch_id IS NULL OR branch_id=?) AND ?::text IS NULL AND ${assignmentReadSql("quality_file_source","quality.read")} AND ${assignmentReadSql("quality_file_source",scope.permission)})`,[scope.targetId,scope.companyId,scope.witness,actor.branchId,actor.branchId,actor.departmentId,actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];});
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function qualityAttachmentFailure(error:unknown){let current=error;for(let depth=0;depth<5&&current&&typeof current==="object";depth++){const item=current as {code?:string;detail?:string;cause?:unknown};if(item.code==="23514"&&item.detail?.includes("quality-file-"))return new ApiError(409,"QUALITY_ATTACHMENT_CHANGED","The original Quality file source or current authority changed.");current=item.cause;}return null;}
