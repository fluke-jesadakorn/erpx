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
import {crmPurchaseAccountRows,crmPurchaseOpportunityRows,crmPurchaseSourceReadLocks,requireCRMCurrentAuthority} from "./purchases";

type Target={kind:"account"|"opportunity";id:string};
export type CRMAttachmentPurchase={companyId:string;capability:"sales.crm";purpose:CapabilityPurpose;actor:WorkContext;target:Target;witness:string};
function sourceQuery(kind:Target["kind"]){return kind==="account"?`SELECT jsonb_build_object('account',to_jsonb(account)-'version'-'updated_at','native',native.witness)::text
 FROM crm_accounts account JOIN (${crmPurchaseAccountRows}) native ON native.id=account.id AND native.company_id=account.company_id
 WHERE account.id=? AND account.company_id=? AND (?::text IS NULL OR account.created_by=? OR EXISTS(SELECT 1 FROM crm_leads lead WHERE lead.account_id=account.id AND lead.company_id=account.company_id AND lead.branch_id=? AND (?::text IS NULL OR lead.department_id=?)))`
 :`SELECT jsonb_build_object('opportunity',to_jsonb(opportunity)-'version'-'updated_at','lead',to_jsonb(lead)-'version'-'updated_at','native',native.witness)::text
 FROM crm_opportunities opportunity JOIN crm_leads lead ON lead.id=opportunity.lead_id JOIN (${crmPurchaseOpportunityRows}) native ON native.id=lead.id AND native.company_id=lead.company_id
 WHERE opportunity.id=? AND lead.company_id=? AND (?::text IS NULL OR lead.branch_id=?) AND (?::text IS NULL OR lead.department_id=?)`;}
function sourceArgs(actor:WorkContext,target:Target){return target.kind==="account"?[target.id,actor.companyId,actor.branchId,actor.userId,actor.branchId,actor.departmentId,actor.departmentId]:[target.id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId];}
export async function crmAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,"targetType"|"purposeKey">,id:string,purpose:CapabilityPurpose="settle-existing"):Promise<CRMAttachmentPurchase|null>{
 if(!["crm_account","crm_opportunity"].includes(policy.targetType)||await commercePackageEnforcementMode()==="disabled")return null;
 const target:Target={kind:policy.targetType==="crm_account"?"account":"opportunity",id},row=await database.prepare(`SELECT (${sourceQuery(target.kind)}) witness`).bind(...sourceArgs(actor,target)).first<{witness:string|null}>();
 if(!row?.witness)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","The original CRM file source is unavailable in this Company.");
 return {companyId:actor.companyId,capability:"sales.crm",purpose,actor,target,witness:row.witness};
}
export async function authorizeCRMAttachmentPurchase(database:SqlDatabase,actor:WorkContext,policy:AttachmentPolicy,id:string,operation:AttachmentOperation){
 const scope=await crmAttachmentPurchaseScope(database,actor,policy,id,operation==="read"?"history":"settle-existing");if(!scope)return;
 await requireCRMCurrentAuthority(database,actor,[operation==="read"?"commerce.read":"commerce.sales"],`crm-file-read:${randomUUID()}`);
 await checkPurchasedCapability(database,await commercePackageEnforcementMode(),scope);
}
export async function crmAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly CRMAttachmentPurchase[]){
 if(!scopes.length||await commercePackageEnforcementMode()==="disabled")return [];
 const locks=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(companyId=>companyPurchaseReadLocks(database,companyId));
 return [...locks.map(items=>items[0]),...crmPurchaseSourceReadLocks(database),...locks.flatMap(items=>items.slice(1))];
}
export async function crmAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly CRMAttachmentPurchase[]){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled"||!scopes.length)return undefined;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.target.kind}:${scope.target.id}:${scope.purpose}`,scope])).values()],boundaries=unique.map(scope=>{const key=`crm-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const guards=(phase:string)=>unique.flatMap(scope=>{const actor=scope.actor,key=`crm-file-${phase}:${randomUUID()}`;return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,"crm-file-authority",`(SELECT ${assignmentReadSql("crm_file_scope","commerce.sales")} FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) crm_file_scope)`,[actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]),assertion(database,key,"crm-file-source",`(${sourceQuery(scope.target.kind)})::jsonb=?::jsonb`,[...sourceArgs(actor,scope.target),scope.witness]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];});
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function crmAttachmentFailure(error:unknown){let cause=error;for(let depth=0;depth<5&&cause&&typeof cause==="object";depth++){const value=cause as {code?:string;detail?:string;cause?:unknown};if(value.code==="23514"&&value.detail?.includes("crm-file-")){const failure=new ApiError(409,"CRM_ATTACHMENT_CHANGED","The original CRM file source or authority changed. Refresh before retrying.");failure.cause=cause;return failure;}cause=value.cause;}return null;}
