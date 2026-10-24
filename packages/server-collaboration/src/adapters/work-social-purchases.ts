import{current,actualSources,capabilities,requireWorkSocialDraftRead,type Source,type Command}from '@erpx/server-collaboration/adapters/work-social-draft-policy';
export{current,actualSources,capabilities,requireWorkSocialDraftRead};
export type{Source,Command};
import { actorScope, projectRead, purchasedReadWhere, pmsRead, socialOwnerModes, requireWorkSocialRead, actorKey, clock } from '@erpx/server-collaboration/adapters/work-social-read-policy';
import{units,type SocialUnit}from '@erpx/server-collaboration/adapters/work-social-unit-state';
import 'server-only';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {socialUploadCleanupFileLocks,beginSocialUploadCleanupFiles,assertSocialUploadCleanupFiles} from '@erpx/server-files/public/social-upload-cleanup';
import { randomUUID } from 'node:crypto';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {isTransactionalSqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {StaffActor} from '@erpx/server-operations/public/staff-policy';
import {workOperationsSettlementSources,lockWorkOperationsSettlementSources} from '@erpx/server-operations/public/work-package-obligation-sources';

import { projectSocialPurchaseSources, projectSocialSourceReadLocks } from '@erpx/server-projects/public/social-purchase-sources';
import { socialProjectAudienceSource } from '@erpx/server-projects/public/collaboration-home-sources';
import {socialPmsSourceFacts,socialPmsSourceReadLocks} from '@erpx/server-pms/public/social-source-facts';
import { pmsSocialSourcePurchaseScopes } from '@erpx/server-pms/public/social-source-purchases';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";

import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';

import {executeCommand,replayCommand} from '@erpx/server-workflow/public/commands';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import { checkPurchasedCapability, companyPurchaseReadLocks, purchaseCommandBoundary, purchaseCommandFailure } from '@erpx/server-commercial/public/purchase-command-boundary';

import { operationalHandoverReceiptPredicate } from '@erpx/server-collaboration/adapters/work-social-source';
import {workTaskSocialFileSourceRows,workTaskSocialAudienceSourceLocks} from '@erpx/server-collaboration/adapters/work-task-purchase-source';
import {sourceAccessSql} from '@erpx/server-collaboration/public/social-source-policy';
import {readablePost} from '@erpx/server-collaboration/adapters/social-source';
import {queuedSocialReplyAuthorityPredicate} from '@erpx/server-identity_access/public/queued-social-source-authority';
import {socialAiProviderSourceReadLocks,socialAiProviderSourceWitness} from '@erpx/server-agent/public/social-ai-source-locks';
import {aiSnapshotGuard} from '@erpx/server-agent/public/ai-providers';
import {socialAgentImageSource,socialAgentImageSourceReadLocks} from '@erpx/server-files/public/social-agent-image-sources';
import type {Reply} from '@erpx/server-collaboration/domain/social-agent';






/** The exact current actor and actual source purchase decision live in each
 * owner SELECT before its native LIMIT/cursor. Callers retain sourceAccessSql. */

/** Product entry always checks actual purchases even during legacy shadow or
 * disabled command rollout; this is not an enforcement-mode mutation. */

/** Minimal presentation flags use the same native source classifier. These
 * are advisory controls; every command repeats its original current guards. */

async function locks(database:SqlDatabase,companyId:string,privateImages=false,providerSources=false,uploadCleanup=false){
 await database.batch([companyPurchaseReadLocks(database,companyId)[0]]);await lockWorkOperationsSettlementSources(database);
 await database.batch([database.prepare('LOCK TABLE social_posts,social_comments,social_post_sources,social_post_revisions,social_task_origins,social_post_drafts,social_post_participants,social_handover_acknowledgements,social_stories,social_reactions,social_follows,social_reports,social_mentions,social_topics,social_post_topics,social_comment_topics,social_ai_drafts,social_ai_usage,social_agent_replies IN SHARE ROW EXCLUSIVE MODE'),workTaskSocialAudienceSourceLocks(database),projectSocialSourceReadLocks(database),socialPmsSourceReadLocks(database)]);if(providerSources)await socialAiProviderSourceReadLocks(database).run();if(privateImages)await socialAgentImageSourceReadLocks(database).run();if(uploadCleanup)await socialUploadCleanupFileLocks(database).run();await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(companyPurchaseReadLocks(database,companyId).slice(1));
}
async function target(database:SqlDatabase,actor:Pick<StaffActor,'companyId'>,command:Command){if(!command.id)return null;const table=command.type==='post'?'social_posts':command.type==='draft'?'social_post_drafts':'social_stories';return database.prepare(`SELECT to_jsonb(row)::text image FROM ${table} row WHERE id=? AND company_id=?`).bind(command.id,actor.companyId).first<string>('image');}

async function actualAudienceProjects(database:SqlDatabase,actor:StaffActor,command:Command){
 if(command.audienceProjectId!==undefined)return command.audienceProjectId?[command.audienceProjectId]:[];
 if(!command.id||command.type==='story')return [];
 const rows=command.type==='post'?await database.prepare("SELECT audience_id id FROM social_posts WHERE id=? AND company_id=? AND audience_kind='project'").bind(command.id,actor.companyId).all<{id:string}>():await database.prepare("SELECT audience_id id FROM social_post_drafts WHERE id=? AND company_id=? AND audience_kind='project' UNION SELECT p.audience_id FROM social_post_drafts d JOIN social_posts p ON p.id=COALESCE(d.published_post_id,d.comment_parent_post_id,d.edit_post_id) AND p.company_id=d.company_id WHERE d.id=? AND d.company_id=? AND p.audience_kind='project'").bind(command.id,actor.companyId,command.id,actor.companyId).all<{id:string}>();
 return [...new Set(rows.results.map(row=>row.id))];
}
async function sourceFacts(database:SqlDatabase,companyId:string,sources:Source[]){return database.prepare(`WITH requested AS(SELECT kind,id FROM jsonb_to_recordset(?::jsonb) AS r(kind text,id text)) SELECT jsonb_build_object('project',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.source_kind,p.source_id) FROM (${projectSocialPurchaseSources}) p JOIN requested r ON r.kind=p.source_kind AND r.id=p.source_id WHERE p.company_id=?),'pms',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM (${socialPmsSourceFacts}) p JOIN requested r ON r.kind='pms_record' AND r.id=p.id WHERE p.company_id=?),'tasks',(SELECT jsonb_agg(jsonb_build_object('task',to_jsonb(t),'social',social.image,'delegations',(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM (${workOperationsSettlementSources.delegations}) d WHERE d.task_id=t.id AND d.company_id=t.company_id)) ORDER BY t.id) FROM (${workOperationsSettlementSources.tasks}) t JOIN requested r ON r.kind='staff_task' AND r.id=t.id LEFT JOIN (${workTaskSocialFileSourceRows}) social ON social.id=t.id AND social.company_id=t.company_id WHERE t.company_id=?))::text image`).bind(JSON.stringify(sources),companyId,companyId,companyId).first<string>('image');}

async function image(database:SqlDatabase,actor:Pick<StaffActor,'companyId'>,command:Command,id?:string){
 const sourceId=id??command.id;if(!sourceId)return null;const postId=command.type==='post'?sourceId:command.type==='draft'?await database.prepare('SELECT COALESCE(published_post_id,comment_parent_post_id,edit_post_id) id FROM social_post_drafts WHERE id=? AND company_id=?').bind(sourceId,actor.companyId).first<string>('id'):null;
 return database.prepare(`SELECT jsonb_build_object('target',?::text,'post',(SELECT to_jsonb(p) FROM social_posts p WHERE p.id=? AND p.company_id=?),'comments',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM social_comments c WHERE c.post_id=?),'sources',(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.ordinal) FROM social_post_sources s WHERE s.post_id=?),'participants',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.user_id) FROM social_post_participants p WHERE p.post_id=?),'revisions',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM social_post_revisions r WHERE r.post_id=?),'reactions',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.principal_id) FROM social_reactions r WHERE r.post_id=?),'follows',(SELECT jsonb_agg(to_jsonb(f) ORDER BY f.principal_id) FROM social_follows f WHERE f.target_type='post' AND f.target_id=?),'drafts',(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM social_post_drafts d WHERE d.company_id=? AND (d.published_post_id=? OR d.comment_parent_post_id=? OR d.edit_post_id=?)),'acknowledgements',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.user_id) FROM social_handover_acknowledgements a WHERE a.post_id=?),'ai_drafts',(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM social_ai_drafts d WHERE d.post_id=?),'ai_usage',(SELECT jsonb_agg(to_jsonb(u) ORDER BY u.id) FROM social_ai_usage u JOIN social_ai_drafts d ON d.id=u.id WHERE d.post_id=?),'agent_replies',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM social_agent_replies r WHERE r.post_id=?))::text image`).bind(await target(database,actor,{...command,id:sourceId}),postId,actor.companyId,postId,postId,postId,postId,postId,postId,actor.companyId,postId,postId,postId,postId,postId,postId,postId).first<string>('image');
}
async function inSocialUnit<Result>(owner:SqlDatabase,actor:StaffActor,run:(database:SqlDatabase)=>Promise<Result>,privateImages=false,providerSources=false,uploadCleanup=false){
 const existing=units.get(owner);if(existing){if(existing.companyId!==actor.companyId||existing.actorKey!==actorKey(actor)||privateImages&&!existing.imagesLeased||providerSources&&!existing.providerLeased)throw new ApiError(503,'WORK_SOCIAL_TRANSACTION_REQUIRED','The original same-context Social unit is required.');return run(owner);}
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'WORK_SOCIAL_TRANSACTION_REQUIRED','Social requires its original complete Holding transaction.');
 try{return await owner.transaction(async database=>{const unit:SocialUnit={providerLeased:providerSources,imagesLeased:privateImages,companyId:actor.companyId,actorKey:actorKey(actor),checks:[],images:[]};units.set(database,unit);try{if(uploadCleanup){await lockPackageSettlementSources(database);await database.prepare('LOCK TABLE active_work_contexts IN EXCLUSIVE MODE').run();await database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE').run();}await locks(database,actor.companyId,privateImages,providerSources,uploadCleanup);const result=await run(database);for(const image of unit.images)image.completed=await image.read();await flushFinanceDeferredEffects(database);for(const check of unit.checks)await check();return result;}finally{units.delete(database);}});}catch(error){throw purchaseCommandFailure(error)??error;}
}
/** Only complete owner transactions are accepted. Registered composition is
 * private and all final checks follow the complete original outer effect set. */
export async function withWorkSocialCommand<Result>(owner:SqlDatabase,actor:StaffActor,command:Command,run:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 const {work:mode,project,disabled}=await socialOwnerModes();if(disabled)return run(owner);
 return inSocialUnit(owner,actor,async database=>{
  const receipt=await database.prepare('SELECT operation,actor_id,record_id FROM command_receipts WHERE command_id=?').bind(command.key).first<{operation:string;actor_id:string;record_id:string}>();
  if(receipt&&(receipt.operation!==command.operation||receipt.actor_id!==actor.userId))throw new ApiError(409,'IDEMPOTENCY_CONFLICT','This command key belongs to another original Social action.');
  const published=command.publishedDraftReplay&&command.id?await database.prepare("SELECT 1 FROM social_post_drafts draft JOIN social_posts post ON post.id=draft.published_post_id AND post.company_id=draft.company_id WHERE draft.id=? AND draft.company_id=? AND draft.author_id=? AND EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.operation='social.post.publish' AND receipt.record_id=post.id AND receipt.actor_id=draft.author_id AND receipt.result_json::jsonb->>'id'=post.id)").bind(command.id,actor.companyId,actor.userId).first():null;
  const retained=command.operation==='work.handover.publish'&&command.id?await database.prepare('SELECT 1 FROM social_posts WHERE id=? AND company_id=? AND author_id=?').bind(command.id,actor.companyId,actor.userId).first():null;
  const purpose=receipt||published||retained?'history':command.purpose,key=`work-social:${randomUUID()}`,sources=await actualSources(database,actor,command),source=await sourceFacts(database,actor.companyId,sources),codes=await capabilities(database,actor,command,sources,purpose),scopes=[...codes.map(capability=>({mode:capability.startsWith('projects.')?project:mode,companyId:actor.companyId,capability,purpose})),...await pmsSocialSourcePurchaseScopes(database,actor,sources.filter(source=>source.kind==='pms_record').map(source=>source.id),purpose)],fences=scopes.flatMap((scope,index)=>{const boundary= purchaseCommandBoundary(database,scope.mode,`${key}:${index}`,scope);return boundary?[{...scope,key:`${key}:${index}`,boundary}]:[];});
  const audiences=await Promise.all((await actualAudienceProjects(database,actor,command)).map(async id=>({id,image:await socialProjectAudienceSource(database,actor,id)})));
  // The original private handover owner hides revoked sources before exposing
  // command authority. Final deferred authority checks still run in this unit.
  if(command.operation==='social.handover.acknowledge'&&command.id)await requireWorkSocialRead(database,actor,command.id);
  await current(database,actor,key);for(const fence of fences)await checkPurchasedCapability(database,fence.mode,{companyId:actor.companyId,capability:fence.capability,purpose});if(command.type==='post'&&command.id&&command.operation!=='work.handover.publish'&&command.operation!=='social.handover.acknowledge')await requireWorkSocialRead(database,actor,command.id);await database.batch(fences.flatMap(fence=>fence.boundary.before));
  const result=await run(database),resultId=result&&typeof result==='object'&&'id' in result&&typeof result.id==='string'?result.id:undefined,completedId=command.id??resultId,unit=units.get(database);if(!unit)throw new ApiError(503,'WORK_SOCIAL_TRANSACTION_REQUIRED','The registered complete Social unit is required.');const snapshot={read:()=>image(database,actor,command,completedId),completed:null as string|null};unit.images.push(snapshot);
  unit.checks.push(async()=>{for(const audience of audiences)if(audience.image!==null&&audience.image!==await socialProjectAudienceSource(database,actor,audience.id))throw new ApiError(409,'PROJECT_AUDIENCE_CHANGED','The original Project audience changed during deferred effects.');if(source!==await sourceFacts(database,actor.companyId,sources)||snapshot.completed!==await snapshot.read())throw new ApiError(409,'WORK_SOCIAL_SOURCE_CHANGED','The completed original Social content or native business source changed during deferred effects.');await current(database,actor,key);await pmsSocialSourcePurchaseScopes(database,actor,sources.filter(source=>source.kind==='pms_record').map(source=>source.id),purpose);const postId=command.type==='post'?completedId:command.type==='draft'&&completedId?await database.prepare('SELECT COALESCE(published_post_id,comment_parent_post_id,edit_post_id) id FROM social_post_drafts WHERE id=? AND company_id=?').bind(completedId,actor.companyId).first<string>('id'):null;if(postId){await requireWorkSocialRead(database,actor,postId);const now=Date.now();if(!await database.prepare(`SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND ${sourceAccessSql('p','?::text')}`).bind(postId,actor.companyId,actor.userId,now,now,actor.assignmentId).first())throw new ApiError(409,'WORK_SOCIAL_SOURCE_CHANGED','The original Social audience or record policy changed during deferred effects.');}const observations=await database.batch(fences.flatMap(fence=>[...fence.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(fence.key)]));fences[0]?.boundary.observe?.(observations);});return result;
 },command.privateImages,command.providerSources);
}

/** This is invoked only by the original Operations typed handover adapter.
 * Freeform Social APIs cannot supply the private marker or mint its receipt. */
export async function withOperationsHandoverPurchaseUnit<Result extends {id:string;version:number}>(owner:SqlDatabase,actor:StaffActor,input:{date:string;body:string;sources:Array<Source&{sourceVersion:number}>;recipientIds:string[]},key:string,run:(database:SqlDatabase)=>Promise<Result>,recoverable=false):Promise<Result>{
 if((await socialOwnerModes()).disabled&&!recoverable)return run(owner);
 return inSocialUnit(owner,actor,async database=>{const unit=units.get(database),sources=input.sources.map(({kind,id})=>({kind,id})),id=`post-post-draft-${key}`;if(!unit||unit.handover)throw new ApiError(503,'WORK_SOCIAL_TRANSACTION_REQUIRED','A fresh original Operations handover unit is required.');unit.handover={key,id,sources};
  // The new recoverable entry records the complete original input even in
  // disabled mode. Old entrypoints retain their existing compatibility path.
  const before=recoverable?await sourceFacts(database,actor.companyId,sources):null,checkKey=`handover-recovery:${randomUUID()}`;if(recoverable)await current(database,actor,checkKey);
  return withWorkSocialCommand(database,actor,{key:`${key}-operations`,operation:'work.handover.publish',type:'post',id,sources,purpose:'new-work'},async tx=>{const result=await run(tx);if(result.id!==id)throw new ApiError(409,'WORK_SOCIAL_SOURCE_CHANGED','The original typed handover identity changed.');await executeCommand(tx,actor,`${key}-operations`,'work.handover.publish',input,id,result.version,{...result,sourceBindings:sources},[]);
   if(recoverable){const read=()=>image(database,actor,{key,operation:'work.handover.publish',type:'post',id,purpose:'history'}),completed={read,completed:null as string|null};unit.images.push(completed);const receipts=()=>database.prepare('SELECT jsonb_agg(to_jsonb(receipt) ORDER BY receipt.command_id)::text image FROM command_receipts receipt WHERE receipt.command_id IN(?,?,?)').bind(key,`${key}-publish`,`${key}-operations`).first<string>('image'),originalReceipts=await receipts();await readablePost(database,actor,id);unit.checks.push(async()=>{if(before!==await sourceFacts(database,actor.companyId,sources)||completed.completed!==await read()||originalReceipts!==await receipts())throw new ApiError(409,'WORK_SOCIAL_SOURCE_CHANGED','The original recovered handover source or immutable receipt changed during deferred effects.');await current(database,actor,checkKey);await readablePost(database,actor,id);});}
   return result;});
 });
}
export function withOperationsHandoverAcknowledgement<Result>(owner:SqlDatabase,actor:StaffActor,id:string,key:string,run:(database:SqlDatabase)=>Promise<Result>){return withWorkSocialCommand(owner,actor,{key,operation:'social.handover.acknowledge',type:'post',id,purpose:'settle-existing'},run);}

/** Reserve/complete each native AI phase in its own owner unit. External
 * provider IO occurs between these calls, never inside a Holding transaction. */
export async function withWorkSocialAiPhase<Result>(owner:SqlDatabase,actor:StaffActor,phase:import("@erpx/server-collaboration/domain/social-ai-phase").SocialAiPhase,run:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 const {disabled}=await socialOwnerModes();if(disabled)return run(owner);
 const key=`social-ai:${phase.reservationId}:${phase.phase}`;
 const aiRows=(database:SqlDatabase)=>database.prepare(`SELECT jsonb_build_object('draft',(SELECT to_jsonb(d) FROM social_ai_drafts d WHERE d.id=?),'usage',(SELECT to_jsonb(u) FROM social_ai_usage u WHERE u.id=?),'reply',(SELECT to_jsonb(r) FROM social_agent_replies r WHERE r.id=?))::text image`).bind(phase.reservationId,phase.reservationId,phase.reply?.id??null).first<string>('image');
 const identity=(database:SqlDatabase)=>database.prepare(`SELECT jsonb_build_object('draft',(SELECT to_jsonb(d)-ARRAY['state','result_json'] FROM social_ai_drafts d WHERE d.id=?),'usage',(SELECT to_jsonb(u)-ARRAY['state','input_tokens','output_tokens','lease_until'] FROM social_ai_usage u WHERE u.id=?),'reply',(SELECT to_jsonb(r)-ARRAY['state','error','error_code','lease_until','updated_at'] FROM social_agent_replies r WHERE r.id=?))::text image`).bind(phase.reservationId,phase.reservationId,phase.reply?.id??null).first<string>('image');
 const pending=async(database:SqlDatabase)=>{
  const row=await database.prepare(`SELECT d.actor_id,d.post_id,d.source_hash,d.actor_json,d.state draft_state,u.company_id,u.actor_id usage_actor,u.state usage_state FROM social_ai_drafts d JOIN social_ai_usage u ON u.id=d.id WHERE d.id=?`).bind(phase.reservationId).first<{actor_id:string;post_id:string;source_hash:string;actor_json:string|null;draft_state:string;company_id:string;usage_actor:string;usage_state:string}>();
  if(phase.phase==='reserve'){if(row)throw new ApiError(409,'AI_REQUEST_EXISTS','This original reservation already exists.');}
  else if(row){if(row.actor_id!==actor.userId||row.usage_actor!==actor.userId||row.company_id!==actor.companyId||row.post_id!==phase.postId||(phase.phase==='failure'?!['pending','failed'].includes(row.draft_state)||!['reserved','failed'].includes(row.usage_state):row.draft_state!=='pending'||row.usage_state!=='reserved')||phase.sourceHash&&row.source_hash!==phase.sourceHash||row.actor_json!==JSON.stringify({assignmentId:actor.assignmentId,role:actor.role,authority:actor.authority}))throw new ApiError(409,'AI_RESERVATION_CHANGED','The original AI reservation changed.');}
  else if(phase.phase!=='failure'||!phase.reply)throw new ApiError(409,'AI_RESERVATION_CHANGED','The original pending AI reservation is required.');
  if(phase.reply&&phase.phase!=='failure'&&!await database.prepare(`SELECT 1 FROM social_agent_replies pending JOIN social_posts p ON p.id=pending.post_id AND p.company_id=pending.company_id WHERE pending.id=? AND pending.company_id=? AND pending.actor_id=? AND pending.post_id=? AND pending.state='running' AND pending.lease_token=? AND pending.actor_json=? AND pending.owner_type=? AND pending.owner_id=? AND p.state='published' AND (${queuedQuestionSourcePredicate()})`).bind(phase.reply.id,actor.companyId,actor.userId,phase.postId,phase.reply.leaseToken,phase.reply.actorJson,phase.reply.ownerType,phase.reply.ownerId).first())throw new ApiError(409,'AI_REPLY_LEASE_CHANGED','The original assistant lease changed.');
 };
 const providerCurrent=async(database:SqlDatabase)=>{if(!phase.provider||phase.phase==='failure')return;if(phase.provider.companyId!==actor.companyId)throw new ApiError(409,'AI_PROVIDER_CHANGED','The original provider Company changed.');await database.batch([aiSnapshotGuard(database,key,phase.provider),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);};
 const perform=async(database:SqlDatabase)=>{
  await pending(database);await providerCurrent(database);const providerImage=phase.provider&&phase.phase!=='failure'?await socialAiProviderSourceWitness(database,phase.provider):null;const ownedFailureReply=phase.phase==='failure'&&phase.reply?await database.prepare("SELECT 1 FROM social_agent_replies WHERE id=? AND company_id=? AND actor_id=? AND post_id=? AND lease_token=? AND actor_json=? AND owner_type=? AND owner_id=? AND state IN ('running','failed')").bind(phase.reply.id,actor.companyId,actor.userId,phase.postId,phase.reply.leaseToken,phase.reply.actorJson,phase.reply.ownerType,phase.reply.ownerId).first():null;const before=phase.phase==='reserve'?null:await identity(database),readImages=async()=>Promise.all((phase.images?.ids??[]).map(id=>socialAgentImageSource(database,{id,companyId:actor.companyId,postId:phase.postId,commentId:phase.images?.commentId??null}))),imageSources=phase.phase==='images'?await readImages():null,readOnlySource=phase.phase==='images'?await image(database,actor,{key,operation:'social.ai.images',type:'post',id:phase.postId,purpose:'settle-existing'}):null,readOnlyAi=phase.phase==='images'?await aiRows(database):null;if(imageSources?.some(row=>!row))throw new ApiError(404,'AI_IMAGE_ACCESS_CHANGED','An image is no longer available.');const result=await run(database);
  if(phase.phase==='reserve')await database.prepare('UPDATE social_ai_drafts SET actor_json=? WHERE id=? AND actor_id=? AND post_id=?').bind(JSON.stringify({assignmentId:actor.assignmentId,role:actor.role,authority:actor.authority}),phase.reservationId,actor.userId,phase.postId).run();
  if(before!==null&&before!==await identity(database))throw new ApiError(409,'AI_RESERVATION_CHANGED','The original AI reservation identity changed during completion.');
  const unit=units.get(database);if(!unit)throw new ApiError(503,'WORK_SOCIAL_TRANSACTION_REQUIRED','The original AI phase requires its complete owner unit.');const snapshot={read:()=>aiRows(database),completed:null as string|null};unit.images.push(snapshot);
  unit.checks.push(async()=>{if(phase.provider&&phase.phase!=='failure'&&providerImage!==await socialAiProviderSourceWitness(database,phase.provider))throw new ApiError(409,'AI_PROVIDER_CHANGED','The original selected provider changed during the native AI phase.');await providerCurrent(database);if(readOnlySource!==null&&(readOnlySource!==await image(database,actor,{key,operation:'social.ai.images',type:'post',id:phase.postId,purpose:'settle-existing'})||readOnlyAi!==await aiRows(database)))throw new ApiError(409,'AI_IMAGE_ACCESS_CHANGED','The original AI source or reservation changed while its private bytes were buffered.');if(imageSources&&JSON.stringify(imageSources)!==JSON.stringify(await readImages()))throw new ApiError(409,'AI_IMAGE_ACCESS_CHANGED','The private image source changed while its bytes were buffered.');if(snapshot.completed!==await snapshot.read())throw new ApiError(409,'AI_RESERVATION_CHANGED','The completed AI reservation or reply changed during deferred effects.');const expected=phase.phase==='finish'?'ready':phase.phase==='failure'?'failed':'pending',usage=phase.phase==='finish'?'succeeded':phase.phase==='failure'?'failed':'reserved';if(!await database.prepare(`SELECT 1 WHERE (?='failure' AND NOT EXISTS(SELECT 1 FROM social_ai_drafts WHERE id=?)) OR EXISTS(SELECT 1 FROM social_ai_drafts d JOIN social_ai_usage u ON u.id=d.id WHERE d.id=? AND d.actor_id=? AND d.post_id=? AND u.company_id=? AND u.actor_id=? AND d.state=? AND u.state=? AND (?<>'failure' OR d.result_json IS NULL) AND (?<>'reserve' OR d.source_hash=?))`).bind(phase.phase,phase.reservationId,phase.reservationId,actor.userId,phase.postId,actor.companyId,actor.userId,expected,usage,phase.phase,phase.phase,phase.sourceHash??null).first())throw new ApiError(409,'AI_RESERVATION_CHANGED','The original AI phase did not complete its expected state.');if(phase.reply&&(phase.phase!=='failure'||ownedFailureReply)&&!await database.prepare("SELECT 1 FROM social_agent_replies WHERE id=? AND company_id=? AND actor_id=? AND post_id=? AND lease_token=? AND actor_json=? AND owner_type=? AND owner_id=? AND state=? AND (? IN ('reserve','images') OR lease_until=0) AND (?='failure' OR state='completed' OR lease_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)").bind(phase.reply.id,actor.companyId,actor.userId,phase.postId,phase.reply.leaseToken,phase.reply.actorJson,phase.reply.ownerType,phase.reply.ownerId,phase.phase==='reserve'||phase.phase==='images'?'running':phase.phase==='finish'?'completed':'failed',phase.phase,phase.phase).first())throw new ApiError(409,'AI_REPLY_LEASE_CHANGED','The original assistant lease did not complete its expected state.');});return result;
 };
 // Closing the exact failed native lease creates no content or permission. It
 // remains possible after revocation, so provider-billed usage cannot strand.
 if(phase.phase==='failure')return inSocialUnit(owner,actor,perform);
 return withWorkSocialCommand(owner,actor,{key,operation:`social.ai.${phase.phase}`,type:'post',id:phase.postId,purpose:phase.phase==='reserve'&&!phase.reply?'new-work':'settle-existing',...(phase.phase==='images'?{privateImages:true as const}:{}),...(phase.provider?{providerSources:true as const}:{})},perform);
}

/** Fixed actual pending reply alias, stored server-captured authority only.
 * No worker role selection or recipient approximation can admit a legacy row. */
function queuedQuestionSourcePredicate(){return `pending.owner_type='post' AND pending.owner_id=p.id AND p.author_id=pending.actor_id OR pending.owner_type='comment' AND EXISTS(SELECT 1 FROM social_comments trigger WHERE trigger.id=pending.owner_id AND trigger.post_id=p.id AND trigger.author_id=pending.actor_id AND trigger.state='published')`;}
function queuedReplyReadPredicate(modes:Awaited<ReturnType<typeof socialOwnerModes>>){
 const project=modes.project==='enforced',pms=modes.pms==='enforced',work=modes.work==='enforced',purchase=work?purchasedReadWhere(project,false,operationalHandoverReceiptPredicate('p'),'settle-existing'):'TRUE';
 const actor="pending.actor_json::jsonb",now=clock;
 return `EXISTS(SELECT 1 FROM social_posts p WHERE p.id=pending.post_id AND p.company_id=pending.company_id AND p.state='published' AND (${queuedQuestionSourcePredicate()}) AND EXISTS(WITH social_purchase_actor AS MATERIALIZED(SELECT ${actor}->>'userId' user_id,${actor}->>'assignmentId' assignment_id,${actor}->>'role' role,pending.company_id,${now} now_ms WHERE ${queuedSocialReplyAuthorityPredicate()}) SELECT 1 FROM social_purchase_actor WHERE ${scheduledFeaturePredicate('social_purchase_actor.company_id','agent')} AND (${purchase}) AND ${project?projectRead('settle-existing',true):'TRUE'} AND ${pms?pmsRead('settle-existing'):'TRUE'} AND ${sourceAccessSql('p','social_purchase_actor.assignment_id',{userId:'social_purchase_actor.user_id',now:'social_purchase_actor.now_ms'})}))`;
}
/** Claim is its own short original Holding transaction. All source leases
 * precede its first snapshot; provider work is never part of this unit. */
export async function withWorkSocialQueueClaim(owner:SqlDatabase,input:{token:string;leaseUntil:number;now:number;postId:string|null},baseline:(database:SqlDatabase)=>Promise<Reply|null>):Promise<Reply|null>{
 const modes=await socialOwnerModes();if(![modes.work,modes.project,modes.pms].includes('enforced'))return baseline(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'WORK_SOCIAL_TRANSACTION_REQUIRED','The queued assistant needs its original Holding transaction.');
 const predicate=queuedReplyReadPredicate(modes);
 try{return await owner.transaction(async database=>{
  await locks(database,'');
  const row=await database.prepare(`UPDATE social_agent_replies SET state='running',attempt=attempt+1,lease_token=?,lease_until=?,updated_at=? WHERE id=(SELECT id FROM social_agent_replies pending WHERE pending.state='pending' AND (?::text IS NULL OR pending.post_id=?) AND ${predicate} ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *`).bind(input.token,input.leaseUntil,input.now,input.postId,input.postId).first<Reply>();if(!row)return null;
  const actor=JSON.parse(row.actor_json) as StaffActor,sources=await actualSources(database,actor,{key:row.id,operation:'social.agent.claim',type:'post',id:row.post_id,purpose:'settle-existing'}),source=await sourceFacts(database,actor.companyId,sources),command:Command={key:row.id,operation:'social.agent.claim',type:'post',id:row.post_id,purpose:'settle-existing'},completed=await image(database,actor,command);
  await flushFinanceDeferredEffects(database);
  if(source!==await sourceFacts(database,actor.companyId,sources)||completed!==await image(database,actor,command)||!await database.prepare(`SELECT 1 FROM social_agent_replies pending WHERE pending.id=? AND pending.state='running' AND pending.lease_token=? AND ${predicate}`).bind(row.id,input.token).first())throw new ApiError(409,'WORK_SOCIAL_SOURCE_CHANGED','The original queued assistant source or authority changed during claim.');
  return row;
 });}catch(error){throw purchaseCommandFailure(error)??error;}
}

/** The original queue may finish a request without generating content when
 * its stored authority/source is no longer claimable. Failed native history
 * then supports the original requester-only explicit retry after recovery. */
export async function withWorkSocialQueueMaintenance<Result>(owner:SqlDatabase,now:number,expire:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 const modes=await socialOwnerModes();if(![modes.work,modes.project,modes.pms].includes('enforced'))return expire(owner);
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'WORK_SOCIAL_TRANSACTION_REQUIRED','Queued assistant cleanup requires its original Holding unit.');
 const predicate=queuedReplyReadPredicate(modes);
 return owner.transaction(async database=>{
  await locks(database,'');const result=await expire(database);
  const picked=(await database.prepare(`SELECT pending.* FROM social_agent_replies pending WHERE pending.state='pending' AND NOT (${predicate}) ORDER BY pending.created_at,pending.id FOR UPDATE SKIP LOCKED LIMIT 500`).all<Reply>()).results;
  const uniquePosts=[...new Map(picked.map(row=>[JSON.stringify([row.company_id,row.post_id]),row])).values()];const sources=await Promise.all(uniquePosts.map(async row=>{const actor={companyId:row.company_id},command:Command={key:row.id,operation:'social.agent.unclaimable',type:'post',id:row.post_id,purpose:'settle-existing'},native=await actualSources(database,actor,command);return {row,actor,command,native,source:await sourceFacts(database,row.company_id,native)};}));
  if(picked.length)await database.batch(picked.map(row=>database.prepare("UPDATE social_agent_replies SET state='failed',error='The conversation or original access is no longer available. Review access, then retry explicitly.',error_code='ACCESS_CHANGED',lease_until=0,updated_at=? WHERE id=? AND company_id=? AND actor_id=? AND post_id=? AND state='pending' AND actor_json=? AND attempt=? AND lease_token IS NOT DISTINCT FROM ?").bind(now,row.id,row.company_id,row.actor_id,row.post_id,row.actor_json,row.attempt,row.lease_token)));
  const completed=await Promise.all(sources.map(source=>image(database,source.actor,source.command)));
  await flushFinanceDeferredEffects(database);
  for(const [index,source] of sources.entries())if(source.source!==await sourceFacts(database,source.row.company_id,source.native)||completed[index]!==await image(database,source.actor,source.command))throw new ApiError(409,'WORK_SOCIAL_SOURCE_CHANGED','The original unclaimable assistant source changed during native cleanup.');
  const finalized=picked.length?await database.prepare(`SELECT count(*)::int n FROM social_agent_replies pending JOIN jsonb_to_recordset(?::jsonb) AS original(id text,company_id text,actor_id text,post_id text,actor_json text,attempt integer,lease_token text) ON original.id=pending.id WHERE pending.company_id=original.company_id AND pending.actor_id=original.actor_id AND pending.post_id=original.post_id AND pending.actor_json=original.actor_json AND pending.attempt=original.attempt AND pending.lease_token IS NOT DISTINCT FROM original.lease_token AND pending.state='failed' AND pending.error_code='ACCESS_CHANGED' AND pending.lease_until=0 AND NOT (${predicate})`).bind(JSON.stringify(picked)).first<number>('n'):0;
  if(finalized!==picked.length)throw new ApiError(409,'WORK_SOCIAL_SOURCE_CHANGED','The original queued authority or no-output terminal state changed during native cleanup.');
  return result;
 });
}

/** Closed native private-file witness. It includes every actual associated
 * post (legacy malformed multi-associations cannot hide a source), and only
 * owner-selected Project/PMS/Task facts; never an input source label. */
export async function workSocialAttachmentSourceImage(database:SqlDatabase,companyId:string,type:'social_draft'|'social_post',id:string){
 const root=type==='social_draft'?'social_post_drafts':'social_posts';
 const row=await database.prepare(`WITH native_root AS(SELECT to_jsonb(root) image FROM ${root} root WHERE root.id=? AND root.company_id=?),
 drafts AS(SELECT d.* FROM social_post_drafts d WHERE d.id=? AND d.company_id=? AND ?='social_draft'),
 post_ids AS(SELECT ?::text id WHERE ?='social_post' UNION SELECT published_post_id FROM drafts UNION SELECT edit_post_id FROM drafts UNION SELECT comment_parent_post_id FROM drafts),
 posts AS(SELECT p.* FROM social_posts p JOIN post_ids selected ON selected.id=p.id WHERE p.company_id=?),
 native_sources AS(SELECT source.source_kind kind,source.source_id id FROM social_post_sources source JOIN posts p ON p.id=source.post_id
 UNION SELECT element->>'kind',element->>'id' FROM drafts d CROSS JOIN LATERAL jsonb_array_elements(CASE WHEN pg_input_is_valid(d.sources_json,'jsonb') THEN CASE WHEN jsonb_typeof(d.sources_json::jsonb)='array' THEN d.sources_json::jsonb ELSE '[]'::jsonb END ELSE '[]'::jsonb END) element WHERE element->>'kind' IN('staff_task','staff_project','pms_record') AND length(element->>'id')>0)
 SELECT jsonb_build_object('root',(SELECT image FROM native_root),'posts',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM posts p),
 'comments',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM social_comments c JOIN posts p ON p.id=c.post_id),
 'sources',(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.post_id,s.ordinal) FROM social_post_sources s JOIN posts p ON p.id=s.post_id),
 'participants',(SELECT jsonb_agg(to_jsonb(member) ORDER BY member.post_id,member.user_id) FROM social_post_participants member JOIN posts p ON p.id=member.post_id),
 'teams',(SELECT jsonb_agg(jsonb_build_object('team',to_jsonb(team),'members',(SELECT jsonb_agg(to_jsonb(member) ORDER BY member.user_id) FROM messaging_team_members member WHERE member.team_id=team.id)) ORDER BY team.id) FROM messaging_teams team WHERE team.company_id=? AND EXISTS(SELECT 1 FROM posts p WHERE p.audience_kind='team' AND p.audience_id=team.id)),
 'revisions',(SELECT jsonb_agg(to_jsonb(revision) ORDER BY revision.id) FROM social_post_revisions revision JOIN posts p ON p.id=revision.post_id))::text image,
 COALESCE((SELECT jsonb_agg(jsonb_build_object('kind',kind,'id',id) ORDER BY kind,id) FROM native_sources),'[]'::jsonb)::text sources`).bind(id,companyId,id,companyId,type,id,type,companyId,companyId).first<{image:string;sources:string}>();
 if(!row)return null;return JSON.stringify([row.image,await sourceFacts(database,companyId,JSON.parse(row.sources) as Source[])]);
}

/** Closing only an original creator's still-unconsumed private upload grants
 * no parent read, Project membership, byte access, publication or new work. */
export async function withWorkSocialUploadCleanup<Result extends {id:string}>(owner:SqlDatabase,actor:StaffActor,input:{id:string;key:string;kind:'comment'|'edit'},run:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 if(units.has(owner))throw new ApiError(503,'WORK_SOCIAL_TRANSACTION_REQUIRED','Upload cleanup requires its fresh complete original owner unit.');
 return inSocialUnit(owner,actor,async database=>{
  const operation=input.kind==='comment'?'social.comment.upload.cancel':'social.post.edit-upload.cancel',createOperation=input.kind==='comment'?'social.comment.upload.create':'social.post.edit-upload.create',prefix=input.kind==='comment'?'comment-upload-':'post-edit-upload-',key=`social-upload-cleanup:${randomUUID()}`;
  await current(database,actor,key);
  const read=()=>database.prepare("SELECT to_jsonb(d)::text draft,(SELECT to_jsonb(c)::text FROM command_receipts c WHERE c.command_id=? AND c.operation=? AND c.record_id=d.id AND c.actor_id=d.author_id AND c.authority_json::jsonb->>'companyId'=d.company_id AND c.result_json::jsonb->>'id'=d.id) creator,EXISTS(SELECT 1 FROM social_comments c WHERE c.comment_upload_draft_id=d.id) consumed FROM social_post_drafts d WHERE d.id=? AND d.company_id=?").bind(input.id.slice(prefix.length),createOperation,input.id,actor.companyId).first<{draft:string;creator:string|null;consumed:boolean}>();
  const before=await read();if(!input.id.startsWith(prefix)||!before?.creator)throw new ApiError(404,'UPLOAD_DRAFT_UNAVAILABLE','This original upload draft is unavailable.');
  type Draft=Record<string,unknown>;const draft=JSON.parse(before.draft) as Draft;
  if(draft.author_id!==actor.userId)throw new ApiError(403,'DRAFT_LOCKED','Only the original author can clean up this private upload.');
  if(before.consumed||draft.published_post_id!==null||draft.published_comment_id!==null||draft.edit_saved_version!==null||(input.kind==='comment'?!draft.comment_parent_post_id||draft.edit_post_id!==null:!draft.edit_post_id||draft.comment_parent_post_id!==null))throw new ApiError(409,'UPLOAD_DRAFT_CONSUMED','This upload has already been consumed by its original conversation.');
  const prior=await database.prepare('SELECT operation,actor_id,record_id,to_jsonb(c)::text image FROM command_receipts c WHERE command_id=?').bind(input.key).first<{operation:string;actor_id:string;record_id:string;image:string}>();
  if(prior&&(prior.operation!==operation||prior.actor_id!==actor.userId||prior.record_id!==input.id))throw new ApiError(409,'IDEMPOTENCY_CONFLICT','This command key belongs to another original action.');
  if(draft.cancelled_at!==null&&!prior)throw new ApiError(404,'UPLOAD_DRAFT_UNAVAILABLE','This upload draft was already cancelled.');
  const files=await beginSocialUploadCleanupFiles(database,{id:input.id,companyId:actor.companyId,userId:actor.userId,version:Number(draft.version)}),result=await run(database);
  const exactResult=(value:unknown)=>value!==null&&typeof value==='object'&&!Array.isArray(value)&&Object.keys(value).length===1&&Reflect.get(value,'id')===input.id;
  const receiptRow=await database.prepare("SELECT operation,actor_id,record_id,authority_json::jsonb->>'companyId' company_id,result_json,to_jsonb(c)::text image FROM command_receipts c WHERE command_id=?").bind(input.key).first<{operation:string;actor_id:string;record_id:string;company_id:string;result_json:string;image:string}>();
  if(!exactResult(result)||!receiptRow||receiptRow.operation!==operation||receiptRow.actor_id!==actor.userId||receiptRow.record_id!==input.id||receiptRow.company_id!==actor.companyId||!exactResult(JSON.parse(receiptRow.result_json)))throw new ApiError(409,'SOCIAL_UPLOAD_CLEANUP_CHANGED','The original cleanup result or receipt changed.');
  const receipt=receiptRow.image;
  // The original Workflow validator owns the request/actor/authority hash.
  // Its assertion writes, and every remaining cleanup write, precede flush.
  if(!exactResult(await replayCommand(database,actor,input.key,operation,input.id,{id:input.id})))throw new ApiError(409,'SOCIAL_UPLOAD_CLEANUP_CHANGED','The original cleanup receipt cannot be replayed exactly.');
  await current(database,actor,key);const completed=await read();if(!completed||receipt!==await database.prepare('SELECT to_jsonb(c)::text image FROM command_receipts c WHERE command_id=?').bind(input.key).first<string>('image'))throw new ApiError(409,'SOCIAL_UPLOAD_CLEANUP_CHANGED','The original cleanup did not complete with its exact receipt.');
  const after=JSON.parse(completed.draft) as Draft,cancelledAt=Number(after.cancelled_at),expected=prior?draft:{...draft,cancelled_at:cancelledAt,updated_at:cancelledAt,version:Number(draft.version)+1};
  if(!Number.isSafeInteger(cancelledAt)||cancelledAt<=0||JSON.stringify(expected)!==JSON.stringify(after)||completed.creator!==before.creator||completed.consumed)throw new ApiError(409,'SOCIAL_UPLOAD_CLEANUP_CHANGED','The original draft changed beyond its cancellation.');
  await assertSocialUploadCleanupFiles(database,files,prior?null:cancelledAt);
  const unit=units.get(database)!;unit.checks.push(async()=>{
   const scope=actorScope(actor);if(!await database.prepare(`${scope.prefix}SELECT 1 FROM social_purchase_actor WHERE ${scheduledFeaturePredicate('company_id','collaboration')}`).bind(...scope.args).first())throw new ApiError(403,'WORK_SOCIAL_AUTHORITY_CHANGED','The original current Social account or role changed.');
   if(JSON.stringify(completed)!==JSON.stringify(await read())||receipt!==await database.prepare('SELECT to_jsonb(c)::text image FROM command_receipts c WHERE command_id=?').bind(input.key).first<string>('image'))throw new ApiError(409,'SOCIAL_UPLOAD_CLEANUP_CHANGED','The original draft or receipt changed during deferred cleanup.');
   await assertSocialUploadCleanupFiles(database,files,prior?null:cancelledAt);
  });return result;
 },false,false,true);
}
