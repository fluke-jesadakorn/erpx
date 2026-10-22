import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import {requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {iamMutationGuards} from '@erpx/server-identity_access/public/iam-store';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {AttachmentOperation,AttachmentPolicy} from '@erpx/server-files/public/attachments/types';
import type {CapabilityPurpose} from '@erpx/server-commercial/public/rules/packages';
import {commercePackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults,companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from '@erpx/server-workflow/public/commands';
import {financeDeferredEffectStatements} from '@erpx/server-finance/public/finance-transactions';
import {scheduledFeaturePredicate} from '@erpx/server-pms/public/hospitality-rollout';
import {hasModuleAccess} from '@erpx/catalogue/module-access';
import {ecommerceCommerceSourceReadLocks} from '@erpx/server-ecommerce/adapters/commerce-source-locks';

export type EcommerceAttachmentPurchase={companyId:string;capability:'sales.ecommerce';purpose:CapabilityPurpose;actor:WorkContext;id:string;witness:string;approverRequired:boolean};
const sourceQuery=`SELECT jsonb_build_array(to_jsonb(listing)-'version'-'updated_at',to_jsonb(channel),product.id,product.company_id)::text witness FROM ecommerce_listings listing JOIN ecommerce_channels channel ON channel.id=listing.channel_id JOIN commerce_products product ON product.id=listing.product_id AND product.company_id=channel.company_id WHERE listing.id=? AND channel.company_id=?`;
export async function ecommerceAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,'targetType'|'purposeKey'>,id:string,operation?:AttachmentOperation):Promise<EcommerceAttachmentPurchase|null>{
 if(policy.targetType!=='ecommerce_listing'||await commercePackageEnforcementMode()==='disabled')return null;
 const row=await database.prepare(sourceQuery).bind(id,actor.companyId).first<{witness:string}>(),witness=row?.witness;if(!witness)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The actual Ecommerce listing file source is unavailable in this Company.');
 return {companyId:actor.companyId,capability:'sales.ecommerce',purpose:operation==='read'||operation==='revoke-history'?'history':operation==='withdraw'||operation==='revoke'||policy.purposeKey!=='listing_media'?'settle-existing':'new-work',actor,id,witness,approverRequired:operation==='revoke'||operation==='revoke-history'};
}
export async function authorizeEcommerceAttachmentPurchase(database:SqlDatabase,actor:WorkContext,policy:AttachmentPolicy,id:string,operation:AttachmentOperation){
 const scope=await ecommerceAttachmentPurchaseScope(database,actor,policy,id,operation);if(!scope)return;
 if(!hasModuleAccess(actor.modules,'ecommerce'))throw new ApiError(403,'MODULE_FORBIDDEN','The original selected assignment cannot access Ecommerce.');
 await requireQueuedCommandAuthority(database,actor,operation==='read'?'commerce.read':'commerce.sales');
 const allowed=await database.prepare(`SELECT EXISTS(SELECT 1 FROM assignments assignment WHERE assignment.id=? AND assignment.company_id=? AND (jsonb_array_length(assignment.modules_json::jsonb)=0 OR assignment.modules_json::jsonb @> '["ecommerce"]'::jsonb)) AND ${scheduledFeaturePredicate('ecommerce_file.company_id','ecommerce')} allowed FROM (SELECT ?::text company_id) ecommerce_file`).bind(actor.assignmentId,actor.companyId,actor.companyId).first<boolean>('allowed');if(!allowed)throw new ApiError(403,'ECOMMERCE_ACCESS_CHANGED','The original Ecommerce module or release is unavailable.');
 if(scope.approverRequired){const review=await database.prepare(`SELECT (${assignmentReadSql('ecommerce_file_review','records.review')} OR ${assignmentReadSql('ecommerce_file_review','commerce.approve')}) allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) ecommerce_file_review`).bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first<boolean>('allowed');if(!review)throw new ApiError(403,'PUBLICATION_APPROVER_REQUIRED','An authorized reviewer must revoke public media.');}
 await checkPurchasedCapability(database,await commercePackageEnforcementMode(),scope);
}
/** Lightweight local leases: no Files service or complete Ecommerce application
 * import can create a policy initialization cycle. Listing bind is a writer. */
export async function ecommerceAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly EcommerceAttachmentPurchase[]){
 if(!scopes.length||await commercePackageEnforcementMode()==='disabled')return [];
 const locks=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(company=>companyPurchaseReadLocks(database,company));return [...locks.map(items=>items[0]),...ecommerceCommerceSourceReadLocks(database),database.prepare('LOCK TABLE commerce_products IN SHARE MODE'),...locks.flatMap(items=>items.slice(1))];
}
export async function ecommerceAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly EcommerceAttachmentPurchase[]){
 const mode=await commercePackageEnforcementMode();if(mode==='disabled'||!scopes.length)return undefined;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.id}:${scope.purpose}:${scope.approverRequired}`,scope])).values()],boundaries=unique.map(scope=>{const key=`ecommerce-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const guards=(phase:string)=>unique.flatMap(scope=>{const actor=scope.actor,key=`ecommerce-file-${phase}:${randomUUID()}`;return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,'ecommerce-file-authority',`(SELECT ${assignmentReadSql('ecommerce_file_scope','commerce.sales')} FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) ecommerce_file_scope)`,[actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]),...(scope.approverRequired?[assertion(database,key,'ecommerce-file-review',`(SELECT (${assignmentReadSql('ecommerce_file_review','records.review')} OR ${assignmentReadSql('ecommerce_file_review','commerce.approve')}) FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) ecommerce_file_review)`,[actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId])]:[]),assertion(database,key,'ecommerce-file-module',`EXISTS(SELECT 1 FROM assignments WHERE id=? AND company_id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> '["ecommerce"]'::jsonb)) AND (SELECT ${scheduledFeaturePredicate('ecommerce_file_release.company_id','ecommerce')} FROM (SELECT ?::text company_id) ecommerce_file_release)`,[actor.assignmentId,scope.companyId,scope.companyId]),assertion(database,key,'ecommerce-file-source',`(${sourceQuery})::jsonb=?::jsonb`,[scope.id,scope.companyId,scope.witness]),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)];});
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards('before')],after:[...financeDeferredEffectStatements(database),...guards('after'),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key))],observe:mode==='shadow'?observePurchaseResults:undefined};
}
export function ecommerceAttachmentFailure(error:unknown){let cause=error;for(let depth=0;depth<5&&cause&&typeof cause==='object';depth++){const value=cause as {code?:string;detail?:string;cause?:unknown};if(value.code==='23514'&&value.detail?.includes('ecommerce-file-')){const failure=new ApiError(409,'ECOMMERCE_ATTACHMENT_CHANGED','The actual Ecommerce listing file source or current authority changed. Refresh before retrying.');failure.cause=cause;return failure;}cause=value.cause;}return null;}
