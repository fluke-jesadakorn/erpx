import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {iamMutationGuards} from '@erpx/server-identity_access/public/iam-store';
import {workContextGuards} from '@erpx/server-workflow/public/commands';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {workPackageEnforcementMode,readPurchasedAccess,checkPurchasedCapability,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import type {CapabilityPurpose} from '@erpx/server-commercial/public/rules/packages';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {HandbookPurchaseControls} from '@erpx/server-collaboration/domain/handbook-purchases';
import {packageClockSql,purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';

function changed(){return new ApiError(409,'WORK_HANDBOOK_SOURCE_CHANGED','The original handbook source or current editor authority changed.');}
export function handbookPresentationContext(actor:WorkContext){return createHash('sha256').update(JSON.stringify([actor.userId,actor.assignmentId,actor.role,actor.companyId,actor.branchId,actor.departmentId,actor.authority,actor.iam])).digest('hex');}
async function current(database:SqlDatabase,actor:WorkContext,command=false){
 const scope=currentAssignmentReadScope(actor,'workforce_actor_scope');
 if(!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${scope.where} AND ${scheduledFeaturePredicate('workforce_actor_scope.company_id','collaboration')}`).bind(actor.companyId,actor.branchId,actor.departmentId,...scope.args).first())throw new ApiError(403,'WORK_HANDBOOK_AUTHORITY_CHANGED','Choose a current account with access to this Company handbook.');
 if(command){const key=`handbook-current:${randomUUID()}`;await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);}
}
/** Only minimal controls leave the owner. Original Company membership and
 * owner-appointed editor authority remain separate checks in the service. */
export async function handbookPurchaseAccess(database:SqlDatabase,actor:WorkContext):Promise<HandbookPurchaseControls|undefined>{
 const mode=await workPackageEnforcementMode();if(mode==='disabled')return;
 if(mode==='enforced')await current(database,actor);
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'work.handbook',purpose:'history'});
 const access=await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'work.handbook'});
 return {newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision};
}
/** Fixed native article alias a; filters the actual collection before its
 * original cursor/LIMIT without changing owner-appointed draft visibility. */
export async function handbookReadScope(actor:WorkContext){
 if(await workPackageEnforcementMode()!=='enforced')return {where:'TRUE',args:[] as (string|number|null)[]};
 const current=currentAssignmentReadScope(actor,'workforce_actor_scope');
 return {where:`EXISTS(WITH handbook_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT 1 FROM handbook_clock CROSS JOIN LATERAL(SELECT a.company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where} AND ${scheduledFeaturePredicate('a.company_id','collaboration')} AND ${purchasedCapabilityPredicate('a.company_id','work.handbook','history','handbook_clock.now_ms')})`,args:[actor.branchId,actor.departmentId,...current.args]};
}
/** Legacy generic sources must resolve to an actual readable native article;
 * a notice label never licenses private drafts or a missing source. */
export async function handbookSourceReadScope(actor:WorkContext,surface:'records'|'staff'|'inbox'){
 if(await workPackageEnforcementMode()!=='enforced')return {where:'TRUE',args:[] as (string|number|null)[]};
 const id=surface==='records'?'workspace_records.id':surface==='staff'?'staff_notifications.record_id':'e.source_id',company=surface==='records'?'workspace_records.company_id':surface==='staff'?'staff_notifications.company_id':'e.company_id',scope=await handbookReadScope(actor),actual=`EXISTS(SELECT 1 FROM handbook_articles original_handbook WHERE original_handbook.id=${id})`,visible=`EXISTS(SELECT 1 FROM handbook_articles a WHERE a.id=${id} AND a.company_id=${company} AND ${scope.where} AND ((a.archived=0 AND a.published_revision IS NOT NULL) OR ?::text='holding_owner' OR EXISTS(SELECT 1 FROM handbook_editors editor WHERE editor.company_id=a.company_id AND editor.user_id=?)))`;
 const type=surface==='staff'?'staff_notifications.record_type':'e.source_type';
 return {where:surface==='records'?`(NOT ${actual} OR ${visible})`:`(${type} NOT IN ('handbook','handbook_article') OR ${visible})`,args:[...scope.args,actor.role,actor.userId]};
}
export async function requireHandbookWorkspaceRead(database:SqlDatabase,actor:WorkContext,id:string){
 if(await workPackageEnforcementMode()!=='enforced')return;
 const scope=await handbookSourceReadScope(actor,'records');
 if(!await database.prepare(`SELECT ${scope.where} allowed FROM(SELECT ?::text id,?::text company_id)workspace_records`).bind(...scope.args,id,actor.companyId).first('allowed'))throw new ApiError(404,'ARTICLE_NOT_FOUND','This original handbook source is unavailable.');
}
async function image(database:SqlDatabase,companyId:string,id?:string){
 return database.prepare(`SELECT jsonb_build_object('article',(SELECT to_jsonb(a) FROM handbook_articles a WHERE a.id=? AND a.company_id=?),
 'revisions',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.revision) FROM handbook_revisions r WHERE r.article_id=? AND r.company_id=?),
 'settings',(SELECT to_jsonb(s) FROM handbook_settings s WHERE s.company_id=?),
 'editors',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.user_id) FROM handbook_editors e WHERE e.company_id=?))::text image`).bind(id??null,companyId,id??null,companyId,companyId,companyId).first<string>('image');
}
type Command={key:string;operation:string;id?:string;purpose:CapabilityPurpose};
/** The wrapper's fixed native command supplies its real ID and purpose. The
 * original service still validates exact replay, membership, owner/editor,
 * publication/revision and version guards inside this complete transaction. */
export async function withHandbookPurchasedCommand<Result>(owner:SqlDatabase,actor:WorkContext,command:Command,run:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase)=>Promise<unknown>){
 const mode=await workPackageEnforcementMode();if(mode==='disabled')return run(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'WORK_HANDBOOK_TRANSACTION_REQUIRED','Handbook commands require their complete fresh Holding transaction.');
 try{return await owner.transaction(async database=>{
  const locks=companyPurchaseReadLocks(database,actor.companyId);
  await database.batch([locks[0],database.prepare('LOCK TABLE handbook_articles,handbook_revisions,handbook_settings,handbook_editors IN SHARE ROW EXCLUSIVE MODE')]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(locks.slice(1));
  const replay=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,actor.userId,actor.companyId).first();
  const scope={companyId:actor.companyId,capability:'work.handbook',purpose:replay?'history' as const:command.purpose},key=`handbook-purchase:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,scope)!;
  await checkPurchasedCapability(database,mode,scope);if(mode==='enforced')await current(database,actor,true);await database.batch(boundary.before);
  const result=await run(database),expected=await image(database,actor.companyId,command.id);
  await flushFinanceDeferredEffects(database);
  if(mode==='enforced')await current(database,actor,true);await authorize(database);
  if(await image(database,actor.companyId,command.id)!==expected)throw changed();
  const observed=await database.batch([...boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);boundary.observe?.(observed);
  return result;
 });}catch(error){throw purchaseCommandFailure(error)??error;}
}
