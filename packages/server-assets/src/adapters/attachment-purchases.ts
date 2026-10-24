import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {AttachmentPolicy,AttachmentOperation} from "@erpx/server-files/public/attachments/types";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';
import {financeDeferredEffectStatements} from "@erpx/server-finance/public/finance-transactions";
import {assetsPackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assetOwnerSourceImage,assetOwnerSourceGuards,assetOwnerSourceReadLocks,assetCurrentAuthority} from "@erpx/server-assets/adapters/purchases";
export type AssetAttachmentPurchase={companyId:string;capability:"assets.register"|"assets.maintenance";purpose:CapabilityPurpose;actor:WorkContext;target:{kind:"asset"|"work";id:string};witness:string};
export async function assetAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,"targetType"|"purposeKey">,id:string,purpose:CapabilityPurpose="settle-existing"):Promise<AssetAttachmentPurchase|null>{
 if(!["asset","maintenance_work"].includes(policy.targetType)||await assetsPackageEnforcementMode()==="disabled")return null;const kind=policy.targetType==="asset"?"asset" as const:"work" as const,target={kind,id},witness=await assetOwnerSourceImage(database,actor,target,true);if(!witness)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","The actual Asset or Maintenance file source is unavailable.");return {companyId:actor.companyId,capability:kind==="asset"?"assets.register":"assets.maintenance",purpose,actor,target,witness};
}
export async function authorizeAssetAttachmentPurchase(database:SqlDatabase,actor:WorkContext,policy:AttachmentPolicy,id:string,operation:AttachmentOperation){const scope=await assetAttachmentPurchaseScope(database,actor,policy,id,operation==="read"?"history":"settle-existing");if(scope){await assetCurrentAuthority(database,actor,[operation==="read"?"records.read":"records.write"],`asset-file-read:${randomUUID()}`);await checkPurchasedCapability(database,await assetsPackageEnforcementMode(),scope);}}
export async function assetAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly AssetAttachmentPurchase[]){if(!scopes.length||await assetsPackageEnforcementMode()==="disabled")return [];return [...new Set(scopes.map(scope=>scope.companyId))].sort().flatMap(companyId=>assetOwnerSourceReadLocks(database,companyId));}
export async function assetAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly AssetAttachmentPurchase[]){
 const mode=await assetsPackageEnforcementMode();if(mode==="disabled"||!scopes.length)return undefined;const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.target.kind}:${scope.target.id}:${scope.purpose}`,scope])).values()],boundaries=unique.map(scope=>{const key=`asset-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const guards=(phase:string)=>unique.flatMap(scope=>{const actor=scope.actor,key=`asset-file-${phase}:${randomUUID()}`;return [database.prepare("SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE").bind(actor.assignmentId,actor.companyId),...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,"asset-file-authority",`(SELECT ${assignmentReadSql("asset_file_scope","records.write")} FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) asset_file_scope)`,[actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]),...assetOwnerSourceGuards(database,actor,scope.target,key,scope.witness,true)];});
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function assetAttachmentFailure(error:unknown){let cause=error;for(let depth=0;depth<5&&cause&&typeof cause==="object";depth++){const value=cause as {code?:string;detail?:string;cause?:unknown};if(value.code==="23514"&&value.detail?.includes("asset-file-")){const failure=new ApiError(409,"ASSET_ATTACHMENT_CHANGED","The Asset file source or authority changed. Refresh before retrying.");failure.cause=cause;return failure;}cause=value.cause;}return null;}
