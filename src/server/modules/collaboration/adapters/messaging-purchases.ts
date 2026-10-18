import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '../../identity_access/public/authorization';
import {lockQueuedCommandAuthority,requireQueuedAccountAuthority} from '../../identity_access/public/queued-command-authority';
import {privateFileDenialAuthoritySourceLocks} from '../../identity_access/public/private-file-denial-authority';
import {workPackageEnforcementMode,companyPurchaseReadLocks,checkPurchasedCapability,readPurchasedAccess,purchaseCommandBoundary,purchaseCommandFailure} from '../../commercial/public/purchase-command-boundary';
import type {CapabilityPurpose} from '../../commercial/public/rules/packages';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {scheduledFeaturePredicate} from '../../pms/public/hospitality-rollout';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {attachmentPrivateReadSourceLocks} from '../../files/public/private-attachment-reads';
import type {MessagingPurchaseControls} from '@/lib/collaboration/messaging-purchases';
type Command={key:string;operation:string;id:string;purpose:CapabilityPurpose|((database:SqlDatabase)=>Promise<CapabilityPurpose>);team?:boolean;privateFiles?:boolean;companyScope?:boolean;alwaysOwned?:boolean;isReplay?:(database:SqlDatabase)=>Promise<boolean>};
type Unit={companyId:string;actorKey:string;checks:Array<()=>Promise<void>>;targets:Set<string>};
const units=new WeakMap<SqlDatabase,Unit>();
const actorKey=(actor:WorkContext)=>JSON.stringify([actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam??null]);
export async function requireMessagingCurrentAccount(database:SqlDatabase,actor:WorkContext){if(await workPackageEnforcementMode()!=='disabled')await requireQueuedAccountAuthority(database,actor);}
async function released(database:SqlDatabase,companyId:string){return !!await database.prepare(`SELECT ${scheduledFeaturePredicate('message_release.company_id','collaboration')} allowed FROM (SELECT ?::text company_id) message_release`).bind(companyId).first<boolean>('allowed');}
export async function messagingPurchaseControls(database:SqlDatabase,actor:WorkContext):Promise<MessagingPurchaseControls|undefined>{
 const mode=await workPackageEnforcementMode();if(mode==='disabled')return;await requireMessagingCurrentAccount(database,actor);const access=await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'work.messages'}),ready=await released(database,actor.companyId);return {newWork:ready&&access.newWork,settleExisting:ready&&access.settleExisting,history:ready&&access.history,revision:access.revision};
}
export async function requireMessagingPurchase(database:SqlDatabase,actor:WorkContext,purpose:CapabilityPurpose){const mode=await workPackageEnforcementMode();if(mode==='disabled')return;await requireQueuedAccountAuthority(database,actor);if(!await released(database,actor.companyId))throw new ApiError(403,'FEATURE_COMING_SOON','Messages is not released for this Company.');await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'work.messages',purpose});}
export async function messagingSourceReadLocks(database:SqlDatabase,companyId:string,privateFiles=false){
 const purchase=companyPurchaseReadLocks(database,companyId);await database.batch([purchase[0],database.prepare('LOCK TABLE messaging_teams,messaging_team_members,messaging_rooms,messaging_room_members,messaging_messages,messaging_reads,messaging_files IN SHARE ROW EXCLUSIVE MODE'),...(privateFiles?[attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]:[])]);await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
}
async function image(database:SqlDatabase,companyId:string,id:string,team=false,companyScope=false){return database.prepare(`SELECT jsonb_build_object('team',(SELECT to_jsonb(t) FROM messaging_teams t WHERE t.id=${team?'?':'(SELECT team_id FROM messaging_rooms WHERE id=?)'} AND t.company_id=?),'teamMembers',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_team_members m WHERE m.team_id=${team?'?':'(SELECT team_id FROM messaging_rooms WHERE id=?)'}),'rooms',(SELECT jsonb_agg(jsonb_build_object('room',to_jsonb(r),'members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM messaging_room_members m WHERE m.room_id=r.id),'messages',(SELECT jsonb_agg(to_jsonb(msg) ORDER BY msg.seq) FROM messaging_messages msg WHERE msg.room_id=r.id),'files',(SELECT jsonb_agg(to_jsonb(f) ORDER BY f.id) FROM messaging_files f WHERE f.room_id=r.id)) ORDER BY r.id) FROM messaging_rooms r WHERE ${team?'r.team_id=?':companyScope?'r.company_id=?':'r.id=?'} AND r.company_id=?))::text image`).bind(id,companyId,id,id,companyId).first<string>('image');}
/** Every source operation, including replay, uses the exact original actor in
 * its complete Holding unit. Arbitrary supplied SQL transactions are denied. */
export async function withMessagingPurchasedCommand<Result>(owner:SqlDatabase,actor:WorkContext,command:Command,run:(database:SqlDatabase)=>Promise<Result>){
 const mode=await workPackageEnforcementMode();if(mode==='disabled'&&!command.alwaysOwned)return run(owner);
 const registered=units.get(owner);if(registered){if(registered.companyId!==actor.companyId||registered.actorKey!==actorKey(actor))throw new ApiError(503,'MESSAGING_TRANSACTION_REQUIRED','Messages requires its original complete owner transaction.');return runGuarded(owner,registered);}
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'MESSAGING_TRANSACTION_REQUIRED','Messages requires its complete owned Holding transaction.');
 try{return await owner.transaction(async database=>{await messagingSourceReadLocks(database,actor.companyId,command.privateFiles);const unit:Unit={companyId:actor.companyId,actorKey:actorKey(actor),checks:[],targets:new Set()};units.set(database,unit);try{const result=await runGuarded(database,unit);await flushFinanceDeferredEffects(database);for(const check of unit.checks)await check();return result;}finally{units.delete(database);}});}catch(error){throw purchaseCommandFailure(error)??error;}
 async function runGuarded(database:SqlDatabase,unit:Unit){
  if(!command.companyScope)unit.targets.add(command.id);
  const replay=command.isReplay?await command.isReplay(database):!!await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND record_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,actor.userId,command.id,actor.companyId).first(),purpose=replay?'history' as const:typeof command.purpose==='function'?await command.purpose(database):command.purpose,scope={companyId:actor.companyId,capability:'work.messages',purpose},key=`messages-purchase:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,scope);
  const bound=await requireQueuedAccountAuthority(database,actor);await requireMessagingPurchase(database,bound,purpose);if(boundary)await database.batch(boundary.before);const result=await run(database),completed=await image(database,actor.companyId,command.id,command.team,command.companyScope);
  unit.checks.push(async()=>{if(completed!==await image(database,actor.companyId,command.id,command.team,command.companyScope))throw new ApiError(409,'MESSAGING_SOURCE_CHANGED','The completed original conversation changed during deferred effects.');await requireQueuedAccountAuthority(database,bound);await requireMessagingPurchase(database,bound,purpose);const observations=await database.batch([...(boundary?.after??[]),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);boundary?.observe?.(observations);});return result;
 }
}

export function messagingOwnsAttachmentAction(database:SqlDatabase,actor:WorkContext,id:string){const unit=units.get(database);return !!unit&&unit.companyId===actor.companyId&&unit.actorKey===actorKey(actor)&&unit.targets.has(id);}
