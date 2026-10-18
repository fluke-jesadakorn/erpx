import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {AttachmentOperation} from '../../files/public/attachments/types';
import {ApiError} from '../../runtime/public/http';
import {iamMutationGuards} from '../../identity_access/public/iam-store';
import {assignmentReadSql} from '../../identity_access/public/record-source-authority';
import {assertion,workContextGuards} from '../../workflow/public/commands';
import {financeDeferredEffectStatements} from '../../finance/public/finance-transactions';
import {legalPackageEnforcementMode,checkPurchasedCapability,companyPurchaseReadLocks,purchaseCommandBoundary,observePurchaseResults} from '../../commercial/public/purchase-command-boundary';
import type {CapabilityPurpose} from '../../commercial/public/rules/packages';
import {legalKindCapability,type LegalPurchaseCapability} from '../domain/purchases';
import type {LegalKind} from '../domain/model';
import {legalOwnedAttachmentPurpose} from './purchases';
export type LegalAttachmentPurchase={companyId:string;targetId:string;actor:WorkContext;purpose:CapabilityPurpose;capabilities:LegalPurchaseCapability[];witness:string};
const source=`SELECT i.*,jsonb_build_object('item',to_jsonb(i)-'version'-'updated_at','members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM legal_members m WHERE m.item_id=i.id))::text image FROM legal_items i WHERE id=? AND company_id=?`;
export async function legalAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,type:string,id:string):Promise<LegalAttachmentPurchase|null>{
 if(type!=='legal_item'||await legalPackageEnforcementMode()==='disabled')return null;
 const row=await database.prepare(source).bind(id,actor.companyId).first<{kind:LegalKind;image:string}>();if(!row)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original Legal file source is unavailable.');
 return {companyId:actor.companyId,targetId:id,actor,purpose:legalOwnedAttachmentPurpose(database,actor,id)??'new-work',capabilities:[legalKindCapability(row.kind),'legal.documents'],witness:row.image};
}
export async function authorizeLegalAttachmentPurchase(database:SqlDatabase,actor:WorkContext,type:string,id:string,operation:AttachmentOperation){
 const scope=await legalAttachmentPurchaseScope(database,actor,type,id);if(!scope)return;const key=`legal-file-read:${randomUUID()}`;await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);for(const capability of scope.capabilities)await checkPurchasedCapability(database,await legalPackageEnforcementMode(),{companyId:scope.companyId,capability,purpose:operation==='read'?'history':scope.purpose});
}
export async function legalAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly LegalAttachmentPurchase[]){
 if(!scopes.length||await legalPackageEnforcementMode()==='disabled')return [];
 const locks=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(companyId=>companyPurchaseReadLocks(database,companyId));
 // Source table leases precede the first MVCC snapshot; the original global
 // legal-files lock remains owned by preservation/destruction, without a new
 // shared-advisory upgrade path in File commands.
 return [...locks.map(items=>items[0]),database.prepare('LOCK TABLE legal_items,legal_members,legal_documents IN SHARE ROW EXCLUSIVE MODE'),...locks.flatMap(items=>items.slice(1))];
}
export async function legalAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly LegalAttachmentPurchase[]){
 const mode=await legalPackageEnforcementMode();if(!scopes.length||mode==='disabled')return;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.targetId}:${scope.purpose}`,scope])).values()],boundaries=unique.flatMap(scope=>scope.capabilities.map(capability=>{const key=`legal-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,{companyId:scope.companyId,capability,purpose:scope.purpose})!};}));
 const guards=(phase:string)=>unique.flatMap(scope=>{const a=scope.actor,key=`legal-file-${phase}:${randomUUID()}`;return [...workContextGuards(database,a,key),...iamMutationGuards(database,a,key),assertion(database,key,'legal-file-source',`EXISTS(SELECT 1 FROM (${source}) legal_file_source WHERE image=? AND ${assignmentReadSql('legal_file_source','legal.edit')} AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) AND EXISTS(SELECT 1 FROM legal_members WHERE item_id=legal_file_source.id AND user_id=? AND access='edit'))`,[scope.targetId,scope.companyId,scope.witness,a.assignmentId,a.userId,a.role,a.branchId,a.branchId,a.departmentId,a.departmentId,a.userId]),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)];});
 return {before:[...guards('before'),...boundaries.flatMap(item=>item.boundary.before)],after:[...financeDeferredEffectStatements(database),...guards('after'),...boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)])],observe:observePurchaseResults};
}
export function legalAttachmentFailure(error:unknown){let current=error;for(let index=0;index<5&&current&&typeof current==='object';index++){const value=current as {code?:string;detail?:string;cause?:unknown};if(value.code==='23514'&&value.detail?.includes('legal-file-'))return new ApiError(409,'LEGAL_ATTACHMENT_CHANGED','The original Legal file source or current authority changed.');current=value.cause;}return null;}
