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
import {commercePackageEnforcementMode,companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults} from '@erpx/server-commercial/public/purchase-command-boundary';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';
import {commerceCompleteOwnerSourceReadLocks} from '@erpx/server-commerce/public/owner-source-locks';
import {finalRootOwnerProjectionSources as commerce} from '@erpx/server-commerce/public/final-root-owner-projection-sources';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {customerPortalSourceReadLocks} from '@erpx/server-customer_authority/adapters/portal-source-locks';
import {financeDeferredEffectStatements} from '@erpx/server-finance/public/finance-transactions';
export type PortalAttachmentPurchase={companyId:string;capability:'sales.portal';purpose:CapabilityPurpose;actor:WorkContext;targetId:string;external:boolean;witness:string};
const sourceQuery=(external:boolean)=>`SELECT jsonb_build_array(to_jsonb(b)-'version'-'updated_at',o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.version,o.state${external?',i.binding_id,i.user_id,i.email,i.bound_at':''})::text
 FROM customer_access_bindings b JOIN (${commerce.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id AND o.kind IN ('sales','quote') ${external?"AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')":''}
 ${external?'JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=? AND i.email=?':''}
 WHERE b.id=? AND b.company_id=? ${external?"AND b.status='approved' AND b.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND b.email=?":'AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)'}`;
const sourceArgs=(scope:Pick<PortalAttachmentPurchase,'external'|'actor'|'targetId'|'companyId'>)=>scope.external?[scope.actor.userId,scope.actor.email.toLowerCase(),scope.targetId,scope.companyId,scope.actor.email.toLowerCase()]:[scope.targetId,scope.companyId,scope.actor.branchId,scope.actor.branchId,scope.actor.departmentId,scope.actor.departmentId];
function staffAuthority(scope:PortalAttachmentPurchase){return {where:`(SELECT (${assignmentReadSql('portal_file_staff','commerce.read')} OR ${assignmentReadSql('portal_file_staff','support.read')}) FROM (SELECT o.company_id,o.branch_id,o.department_id FROM customer_access_bindings b JOIN (${commerce.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id WHERE b.id=? AND b.company_id=?) portal_file_staff)`,args:[scope.actor.assignmentId,scope.actor.userId,scope.actor.role,scope.actor.assignmentId,scope.actor.userId,scope.actor.role,scope.targetId,scope.companyId]};}
export async function portalAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,'targetType'|'purposeKey'>,id:string,operation?:AttachmentOperation):Promise<PortalAttachmentPurchase|null>{
 if(policy.targetType!=='customer_access_binding'||await commercePackageEnforcementMode()==='disabled')return null;
 const external=actor.role==='external_customer'&&actor.assignmentId===`customer-portal:${id}`,scope={companyId:actor.companyId,capability:'sales.portal' as const,purpose:operation==='read'||operation==='revoke-history'?'history' as const:'settle-existing' as const,actor,targetId:id,external},row=await database.prepare(`SELECT (${sourceQuery(external)}) witness`).bind(...sourceArgs(scope)).first<{witness:string|null}>();
 if(!row?.witness)throw new ApiError(404,'PORTAL_ATTACHMENT_TARGET_NOT_FOUND','The actual verified customer binding and document are unavailable.');return {...scope,witness:row.witness};
}
export async function authorizePortalAttachmentPurchase(database:SqlDatabase,actor:WorkContext,policy:AttachmentPolicy,id:string,operation:AttachmentOperation){
 const scope=await portalAttachmentPurchaseScope(database,actor,policy,id,operation);if(!scope)return;
 if(!scope.external){if(operation!=='read')throw new ApiError(403,'PORTAL_ATTACHMENT_FORBIDDEN','Only the original verified customer may add evidence.');await requireQueuedCommandAuthority(database,actor,'commerce.read').catch(async error=>{if(error instanceof ApiError&&error.status===403)return requireQueuedCommandAuthority(database,actor,'support.read');throw error;});const authority=staffAuthority(scope);if(!await database.prepare(`SELECT ${authority.where} allowed`).bind(...authority.args).first<boolean>('allowed'))throw new ApiError(403,'PORTAL_ATTACHMENT_FORBIDDEN','Current staff authority does not permit this customer evidence.');}
 if(!await database.prepare(`SELECT ${scheduledFeaturePredicate('portal_file_release.company_id','customer_authority')} allowed FROM (SELECT ?::text company_id) portal_file_release`).bind(scope.companyId).first<boolean>('allowed'))throw new ApiError(403,'FEATURE_COMING_SOON','Customer access is not released for this Company.');
 await checkPurchasedCapability(database,await commercePackageEnforcementMode(),scope);
}
export async function portalAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly PortalAttachmentPurchase[]){
 if(!scopes.length||await commercePackageEnforcementMode()==='disabled')return [];
 const locks=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(company=>companyPurchaseReadLocks(database,company));return [...locks.map(items=>items[0]),...commerceCompleteOwnerSourceReadLocks(database),...customerPortalSourceReadLocks(database),...locks.flatMap(items=>items.slice(1))];
}
export async function portalAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly PortalAttachmentPurchase[]){
 const mode=await commercePackageEnforcementMode();if(mode==='disabled'||!scopes.length)return undefined;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.targetId}:${scope.purpose}:${scope.actor.userId}`,scope])).values()],boundaries=unique.map(scope=>{const key=`portal-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const guards=(phase:string)=>unique.flatMap(scope=>{const key=`portal-file-${phase}:${randomUUID()}`,authority=staffAuthority(scope);return [assertion(database,key,'portal-file-source',`(${sourceQuery(scope.external)})::jsonb=?::jsonb`,[...sourceArgs(scope),scope.witness]),...(scope.external?[]:[...workContextGuards(database,scope.actor,key),...iamMutationGuards(database,scope.actor,key),assertion(database,key,'portal-file-staff',authority.where,authority.args)]),assertion(database,key,'portal-file-release',`(SELECT ${scheduledFeaturePredicate('portal_file_release.company_id','customer_authority')} FROM (SELECT ?::text company_id) portal_file_release)`,[scope.companyId]),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)];});
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards('before')],after:[...financeDeferredEffectStatements(database),...guards('after'),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key))],observe:mode==='shadow'?observePurchaseResults:undefined};
}
export function portalAttachmentFailure(error:unknown){let cause=error;for(let depth=0;depth<5&&cause&&typeof cause==='object';depth++){const value=cause as {code?:string;detail?:string;cause?:unknown};if(value.code==='23514'&&value.detail?.includes('portal-file-'))return new ApiError(409,'PORTAL_ATTACHMENT_CHANGED','The original customer binding, document, account or package changed. Review before retrying.');cause=value.cause;}return null;}
