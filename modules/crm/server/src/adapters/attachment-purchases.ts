import "server-only";
import { nativeClearCommandAssertions } from '@erpx/collaboration-server/adapters/native-command-assertions';
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {assignmentReadSql} from "@erpx/identity-access-server/public/record-source-authority";
import {iamMutationGuards} from "@erpx/identity-access-server/public/iam-store";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import type {AttachmentOperation,AttachmentPolicy} from "@erpx/files-server/public/attachments/types";
import type {CapabilityPurpose} from "@erpx/commercial-server/public/rules/packages";
import {commercePackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults,companyPurchaseReadLocks} from "@erpx/commercial-server/public/purchase-command-boundary";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {workContextGuards} from '@erpx/workflow-server/public/command-guards';
import {financeDeferredEffectStatements} from "@erpx/finance-server/public/finance-transactions";
import {actorScopeSql,crmPurchaseAccountRows,crmPurchaseOpportunityRows,crmAccountCreatedByScopeSql,crmPurchaseSourceReadLocks,requireCRMCurrentAuthority} from "@erpx/crm-server/adapters/purchases";
import {crmAccountAttachmentSourceSql} from "./read-tail-r29-sql";
import {crmOpportunitySourceAnchorSql} from "./read-tail-r36-sql";
import {crmScalarSubquerySql} from "./read-tail-r43-sql";
import {crmSelectExprSql} from './read-tail-r50-sql';

type Target={kind:"account"|"opportunity";id:string};
export type CRMAttachmentPurchase={companyId:string;capability:"sales.crm";purpose:CapabilityPurpose;actor:WorkContext;target:Target;witness:string};
function sourceQuery(kind:Target["kind"]){return kind==="account"?crmAccountAttachmentSourceSql(crmPurchaseAccountRows,`${crmAccountCreatedByScopeSql}`)
 :crmOpportunitySourceAnchorSql(
   `jsonb_build_object('opportunity',to_jsonb(opportunity)-'version'-'updated_at','lead',to_jsonb(lead)-'version'-'updated_at','native',native.witness)::text\n`,
   ` JOIN (${crmPurchaseOpportunityRows}) native ON native.id=lead.id AND native.company_id=lead.company_id
 WHERE opportunity.id=? AND lead.company_id=? AND (?::text IS NULL OR lead.branch_id=?) AND (?::text IS NULL OR lead.department_id=?)`,
 );}
function sourceArgs(actor:WorkContext,target:Target){return target.kind==="account"?[target.id,actor.companyId,actor.branchId,actor.userId,actor.branchId,actor.departmentId,actor.departmentId]:[target.id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId];}
export async function crmAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,"targetType"|"purposeKey">,id:string,purpose:CapabilityPurpose="settle-existing"):Promise<CRMAttachmentPurchase|null>{
 if(!["crm_account","crm_opportunity"].includes(policy.targetType)||await commercePackageEnforcementMode()==="disabled")return null;
 const target:Target={kind:policy.targetType==="crm_account"?"account":"opportunity",id},row=await database.prepare(crmSelectExprSql(`${sourceQuery(target.kind)}) witness`)).bind(...sourceArgs(actor,target)).first<{witness:string|null}>();
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
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.target.kind}:${scope.target.id}:${scope.purpose}`,scope])).values()],boundaries=unique.map(scope=>{const key=`crm-file-purchase:${randomUUID()}`;return {key,companyId:scope.companyId,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const guards=async (phase:string)=>(await Promise.all(unique.map(async scope=>{const actor=scope.actor,key=`crm-file-${phase}:${randomUUID()}`;return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,"crm-file-authority",crmScalarSubquerySql(assignmentReadSql("crm_file_scope","commerce.sales"),`${actorScopeSql} crm_file_scope`),[actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]),assertion(database,key,"crm-file-source",`(${sourceQuery(scope.target.kind)})::jsonb=?::jsonb`,[...sourceArgs(actor,scope.target),scope.witness]),await nativeClearCommandAssertions(database, actor.companyId, key)];}))).flat();
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...await guards("before")],after:[...financeDeferredEffectStatements(database),...await guards("after"),...boundaries.flatMap(item=>item.boundary.after),...await Promise.all(boundaries.map(item=>nativeClearCommandAssertions(database, item.companyId, item.key)))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function crmAttachmentFailure(error:unknown){let cause=error;for(let depth=0;depth<5&&cause&&typeof cause==="object";depth++){const value=cause as {code?:string;detail?:string;cause?:unknown};if(value.code==="23514"&&value.detail?.includes("crm-file-")){const failure=new ApiError(409,"CRM_ATTACHMENT_CHANGED","The original CRM file source or authority changed. Refresh before retrying.");failure.cause=cause;return failure;}cause=value.cause;}return null;}
