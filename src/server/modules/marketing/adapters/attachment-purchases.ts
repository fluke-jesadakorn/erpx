import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "../../identity_access/public/authorization";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import {iamMutationGuards} from "../../identity_access/public/iam-store";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {ApiError} from "../../runtime/public/http";
import type {AttachmentOperation,AttachmentPolicy} from "../../files/public/attachments/types";
import type {CapabilityPurpose} from "../../commercial/public/rules/packages";
import {commercePackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults,companyPurchaseReadLocks} from "../../commercial/public/purchase-command-boundary";
import {assertion,workContextGuards} from "../../workflow/public/commands";
import {financeDeferredEffectStatements} from "../../finance/public/finance-transactions";
import {marketingPurchaseSourceReadLocks,requireMarketingCurrentAuthority} from "./purchases";

type Target={kind:"asset"|"campaign";id:string};
export type MarketingAttachmentPurchase={companyId:string;capability:"sales.marketing";purpose:CapabilityPurpose;actor:WorkContext;target:Target;witness:string;approverRequired:boolean};
const sourceQuery=(kind:Target["kind"])=>`SELECT (to_jsonb(source)-'version'-'updated_at')::text FROM ${kind==="asset"?"marketing_assets":"marketing_campaigns"} source WHERE source.id=? AND source.company_id=?`;
export async function marketingAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,"targetType"|"purposeKey">,id:string,operation?:AttachmentOperation):Promise<MarketingAttachmentPurchase|null>{
 if(!["marketing_asset","marketing_campaign"].includes(policy.targetType)||await commercePackageEnforcementMode()==="disabled")return null;
 const target:Target={kind:policy.targetType==="marketing_asset"?"asset":"campaign",id},row=await database.prepare(`SELECT (${sourceQuery(target.kind)}) witness`).bind(id,actor.companyId).first<{witness:string|null}>();
 if(!row?.witness)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","The original Marketing file source is unavailable in this Company.");
 return {companyId:actor.companyId,capability:"sales.marketing",purpose:operation==="read"||operation==="revoke-history"?"history":operation==="withdraw"||operation==="revoke"?"settle-existing":(target.kind==="asset"||policy.purposeKey==="creative"?"new-work":"settle-existing"),actor,target,witness:row.witness,approverRequired:operation==="revoke"||operation==="revoke-history"};
}
export async function authorizeMarketingAttachmentPurchase(database:SqlDatabase,actor:WorkContext,policy:AttachmentPolicy,id:string,operation:AttachmentOperation){
 const scope=await marketingAttachmentPurchaseScope(database,actor,policy,id,operation);if(!scope)return;
 await requireMarketingCurrentAuthority(database,actor,[operation==="read"?"records.read":"records.write"],`marketing-file-read:${randomUUID()}`);
 if(scope.approverRequired){
  const current=await database.prepare(`SELECT (${assignmentReadSql("marketing_file_review","records.review")} OR ${assignmentReadSql("marketing_file_review","commerce.approve")}) allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) marketing_file_review`).bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first<{allowed:boolean}>();
  if(!current?.allowed)throw new ApiError(403,"PUBLICATION_APPROVER_REQUIRED","An authorized reviewer must revoke public media.");
 }
 await checkPurchasedCapability(database,await commercePackageEnforcementMode(),scope);
}
export async function marketingAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly MarketingAttachmentPurchase[]){
 if(!scopes.length||await commercePackageEnforcementMode()==="disabled")return [];
 const locks=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(companyId=>companyPurchaseReadLocks(database,companyId));
 return [...locks.map(items=>items[0]),...marketingPurchaseSourceReadLocks(database),...locks.flatMap(items=>items.slice(1))];
}
export async function marketingAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly MarketingAttachmentPurchase[]){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled"||!scopes.length)return undefined;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.target.kind}:${scope.target.id}:${scope.purpose}:${scope.approverRequired}`,scope])).values()],boundaries=unique.map(scope=>{const key=`marketing-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const guards=(phase:string)=>unique.flatMap(scope=>{const actor=scope.actor,key=`marketing-file-${phase}:${randomUUID()}`;return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,"marketing-file-authority",`(SELECT ${assignmentReadSql("marketing_file_scope","records.write")} FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) marketing_file_scope)`,[actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]),...(scope.approverRequired?[assertion(database,key,"marketing-file-reviewer",`(SELECT (${assignmentReadSql("marketing_file_review","records.review")} OR ${assignmentReadSql("marketing_file_review","commerce.approve")}) FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) marketing_file_review)`,[actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId])]:[]),assertion(database,key,"marketing-file-module",`EXISTS(SELECT 1 FROM assignments WHERE id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)))`,[actor.assignmentId]),assertion(database,key,"marketing-file-source",`(${sourceQuery(scope.target.kind)})::jsonb=?::jsonb`,[scope.target.id,scope.companyId,scope.witness]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];});
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function marketingAttachmentFailure(error:unknown){let cause=error;for(let depth=0;depth<5&&cause&&typeof cause==="object";depth++){const value=cause as {code?:string;detail?:string;cause?:unknown};if(value.code==="23514"&&value.detail?.includes("marketing-file-")){const failure=new ApiError(409,"MARKETING_ATTACHMENT_CHANGED","The original Marketing file source or authority changed. Refresh before retrying.");failure.cause=cause;return failure;}cause=value.cause;}return null;}
