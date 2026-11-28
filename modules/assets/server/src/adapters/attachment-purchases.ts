import "server-only";
import { nativeClearCommandAssertions } from '@erpx/collaboration-server/adapters/native-command-assertions';
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import type {AttachmentPolicy,AttachmentOperation} from "@erpx/files-server/public/attachments/types";
import {assignmentReadSql} from "@erpx/identity-access-server/public/record-source-authority";
import {iamMutationGuards} from "@erpx/identity-access-server/public/iam-store";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {workContextGuards} from '@erpx/workflow-server/public/command-guards';
import {financeDeferredEffectStatements} from "@erpx/finance-server/public/finance-transactions";
import {assetsPackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults} from "@erpx/commercial-server/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/commercial-server/public/rules/packages";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {assetOwnerSourceImage,assetOwnerSourceGuards,assetOwnerSourceReadLocks,assetCurrentAuthority} from "@erpx/assets-server/adapters/purchases";
import {assetCurrentScopeProbeSql,assetScopeTupleSql} from "./asset-scope-tuple";
import {assetIdCompanyReadSql} from "./attachments-policy-facts";
export type AssetAttachmentPurchase={companyId:string;capability:"assets.register"|"assets.maintenance";purpose:CapabilityPurpose;actor:WorkContext;target:{kind:"asset"|"work";id:string};witness:string};
export async function assetAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,"targetType"|"purposeKey">,id:string,purpose:CapabilityPurpose="settle-existing"):Promise<AssetAttachmentPurchase|null>{
 if(!["asset","maintenance_work"].includes(policy.targetType)||await assetsPackageEnforcementMode()==="disabled")return null;const kind=policy.targetType==="asset"?"asset" as const:"work" as const,target={kind,id},witness=await assetOwnerSourceImage(database,actor,target,true);if(!witness)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","The actual Asset or Maintenance file source is unavailable.");return {companyId:actor.companyId,capability:kind==="asset"?"assets.register":"assets.maintenance",purpose,actor,target,witness};
}
export async function authorizeAssetAttachmentPurchase(database:SqlDatabase,actor:WorkContext,policy:AttachmentPolicy,id:string,operation:AttachmentOperation){const scope=await assetAttachmentPurchaseScope(database,actor,policy,id,operation==="read"?"history":"settle-existing");if(scope){await assetCurrentAuthority(database,actor,[operation==="read"?"records.read":"records.write"],`asset-file-read:${randomUUID()}`);await checkPurchasedCapability(database,await assetsPackageEnforcementMode(),scope);}}
export async function assetAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly AssetAttachmentPurchase[]){if(!scopes.length||await assetsPackageEnforcementMode()==="disabled")return [];return [...new Set(scopes.map(scope=>scope.companyId))].sort().flatMap(companyId=>assetOwnerSourceReadLocks(database,companyId));}
export async function assetAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly AssetAttachmentPurchase[]){
 const mode=await assetsPackageEnforcementMode();if(mode==="disabled"||!scopes.length)return undefined;const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.target.kind}:${scope.target.id}:${scope.purpose}`,scope])).values()],boundaries=unique.map(scope=>{const key=`asset-file-purchase:${randomUUID()}`;return {key,companyId:scope.companyId,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const guards=async (phase:string)=>(await Promise.all(unique.map(async scope=>{const actor=scope.actor,key=`asset-file-${phase}:${randomUUID()}`;return [database.prepare(assetIdCompanyReadSql("id","assignments"," FOR SHARE")).bind(actor.assignmentId,actor.companyId),...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,"asset-file-authority",`(${assetCurrentScopeProbeSql(assignmentReadSql("asset_file_scope","records.write"),` ${assetScopeTupleSql("asset_file_scope")}`)})`,[actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]),...await assetOwnerSourceGuards(database,actor,scope.target,key,scope.witness,true)];}))).flat();
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...await guards("before")],after:[...financeDeferredEffectStatements(database),...await guards("after"),...boundaries.flatMap(item=>item.boundary.after),...await Promise.all(boundaries.map(item=>nativeClearCommandAssertions(database, item.companyId, item.key)))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function assetAttachmentFailure(error:unknown){let cause=error;for(let depth=0;depth<5&&cause&&typeof cause==="object";depth++){const value=cause as {code?:string;detail?:string;cause?:unknown};if(value.code==="23514"&&value.detail?.includes("asset-file-")){const failure=new ApiError(409,"ASSET_ATTACHMENT_CHANGED","The Asset file source or authority changed. Refresh before retrying.");failure.cause=cause;return failure;}cause=value.cause;}return null;}
