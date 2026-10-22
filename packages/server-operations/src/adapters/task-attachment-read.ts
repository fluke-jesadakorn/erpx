import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {iamMutationGuards} from '@erpx/server-identity_access/public/iam-store';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {privateFileDenialAuthorityWitness,privateFileDenialAuthoritySourceLocks} from '@erpx/server-identity_access/public/private-file-denial-authority';
import {workContextGuards} from '@erpx/server-workflow/public/commands';
import {workTaskApprovalSourceLocks} from '@erpx/server-workflow/public/work-package-obligation-sources';
import {workTaskSocialFileSourceRows,workTaskSocialSourceLocks,workTaskSocialAudienceSourceLocks} from '@erpx/server-collaboration/public/work-task-purchase-source';
import {projectSocialSourceReadLocks,projectTaskAttachmentReadSources} from '@erpx/server-projects/public/social-purchase-sources';
import {socialPmsSourceReadLocks} from '@erpx/server-pms/public/social-source-facts';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {taskPrivateAttachmentSource,type TaskPrivateAttachmentSource} from '@erpx/server-files/public/task-private-read-sources';
import {attachmentReadSource,attachmentPrivateReadSourceLocks,assertAttachmentReadRoute,type AttachmentReadRoute} from '@erpx/server-files/public/private-attachment-reads';
import {attachmentDeniedReadAudits} from '@erpx/server-files/public/attachment-denied-read-audits';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';

async function taskImage(database:SqlDatabase,actor:WorkContext,id:string){
 return database.prepare(`SELECT jsonb_build_object('task',to_jsonb(t),'project',project.image,
 'template',(SELECT to_jsonb(template) FROM routine_templates template WHERE template.id=t.template_id AND template.company_id=t.company_id),
 'schedule',(SELECT to_jsonb(schedule) FROM routine_schedules schedule WHERE schedule.id=t.schedule_id AND schedule.company_id=t.company_id),
 'delegations',(SELECT jsonb_agg(to_jsonb(delegation) ORDER BY delegation.id) FROM staff_task_delegations delegation WHERE delegation.task_id=t.id AND delegation.company_id=t.company_id),
 'social',social.image,'native_social',social.work_social)::text image
 FROM staff_tasks t LEFT JOIN (${projectTaskAttachmentReadSources}) project ON project.task_id=t.id AND project.company_id=t.company_id
 LEFT JOIN (${workTaskSocialFileSourceRows}) social ON social.id=t.id AND social.company_id=t.company_id
 WHERE t.id=? AND t.company_id=?`).bind(id,actor.companyId).first<string>('image');
}
function changed(){return new ApiError(409,'WORK_TASK_FILE_READ_CHANGED','The private task evidence, source or current authority changed while the file was being read.');}
async function current(database:SqlDatabase,actor:WorkContext){
 const key=`work-task-byte-read:${randomUUID()}`;
 try{await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);}
 catch(error){let cause=error;for(let depth=0;depth<5&&cause&&typeof cause==='object';depth++){const item=cause as {code?:string;cause?:unknown};if(item.code==='23514')throw changed();cause=item.cause;}throw error;}
}
const legalParents=new WeakMap<SqlDatabase,{actor:WorkContext;identity:string}>();
const actorIdentity=(actor:WorkContext)=>JSON.stringify([actor.userId,actor.email,actor.assignmentId,actor.role,actor.companyId,actor.holdingId,actor.branchId,actor.departmentId,actor.assignmentScope,actor.scope,actor.modules,actor.authority,actor.iam]);
async function sourceLeases(database:SqlDatabase,companyId:string,readPurchase=true){
 const locks=companyPurchaseReadLocks(database,companyId);
 // All changing source owners precede the first SERIALIZABLE data snapshot.
 await database.batch([locks[0],database.prepare('LOCK TABLE staff_tasks,staff_task_delegations,routine_templates,routine_schedules IN SHARE ROW EXCLUSIVE MODE'),workTaskApprovalSourceLocks(database),workTaskSocialSourceLocks(database),workTaskSocialAudienceSourceLocks(database),projectSocialSourceReadLocks(database),socialPmsSourceReadLocks(database),attachmentPrivateReadSourceLocks(database)]);
 await database.batch(privateFileDenialAuthoritySourceLocks(database));await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);if(readPurchase)await database.batch(locks.slice(1));
}
/** Only the closed fresh Legal preservation constructor can enter this phase.
 * The private SQL identity is retained until Legal flushes its whole command. */
export async function prepareWorkTaskAttachmentParentRead(database:SqlDatabase,actor:WorkContext){
 const legal=await import('@erpx/server-legal/public/attachment-read-parent');legal.requireLegalAttachmentReadPreparation(database,actor.companyId);
 if(legalParents.has(database))throw new ApiError(503,'WORK_TASK_FILE_READ_PREPARATION_REQUIRED','Task evidence preparation may run only once before the Legal source snapshot.');
 await sourceLeases(database,actor.companyId,false);legalParents.set(database,{actor,identity:actorIdentity(actor)});
}
/** Hold the original owner sources through blob consumption and the access
 * audit's deferred effects. An arbitrary caller snapshot cannot inherit this
 * unit. Read policy is the original participant policy, never a File writer
 * policy or an additional People/Work grant on actual Project tasks. */
export async function withWorkTaskAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,
 selectedRoute:AttachmentReadRoute,
 read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:TaskPrivateAttachmentSource)=>Promise<unknown>):Promise<Result>{
 if(selectedRoute.targetType!=='staff_task')throw changed();
 const parent=legalParents.get(owner);
 const inherited=!!parent&&parent.actor===actor&&parent.identity===actorIdentity(actor);
 if(!inherited&&!isTransactionalSqlDatabase(owner))throw new ApiError(503,'WORK_TASK_FILE_READ_TRANSACTION_REQUIRED','Private task bytes require their fresh complete owned read transaction.');
 const context=actorIdentity(actor);
 type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const execute=async(database:SqlDatabase,defer?:(check:()=>Promise<void>)=>void):Promise<Outcome>=>{
  const routed=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selectedRoute,routed);
  const before=await taskPrivateAttachmentSource(database,actor.companyId,linkId);
  if(!before)throw changed();
  const image=await taskImage(database,actor,before.targetId);if(!image)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original task evidence owner is unavailable.');
  const check=async(authorizeSource=true,denial=false)=>{
   if(actorIdentity(actor)!==context)throw changed();
   if(!denial||initiallyCurrent)await current(database,actor);else if(await privateFileDenialAuthorityWitness(database,actor)!==deniedAuthority)throw changed();
   if(authorizeSource)await authorize(database,before);
   const after=await taskPrivateAttachmentSource(database,actor.companyId,linkId);
   const routedAfter=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(routed,routedAfter);
   if(!after||after.witness!==before.witness||routedAfter?.witness!==routed?.witness||await taskImage(database,actor,before.targetId)!==image)throw changed();
  };
  const currentScope=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=!defer&&!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${currentScope.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...currentScope.args).first();
  const deniedAuthority=!defer&&!initiallyCurrent?await privateFileDenialAuthorityWitness(database,actor):null;
  const checkpoint=defer?null:new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id));let result:Result;
  try{result=await read(database);}catch(error){
   // A rejected standalone read may retain only the denial the original Files
   // policy actually wrote. An inherited Legal mutation still rolls back whole.
   if(!checkpoint||!(error instanceof ApiError)||!([403,404].includes(error.status)||(error.status===423&&error.code==='ATTACHMENT_NOT_AVAILABLE')||(error.status===503&&error.code==='FILE_UNAVAILABLE')))throw error;
   const denied=(await attachmentDeniedReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));
   if(denied.length!==1||denied[0].reason!==error.code)throw error;
   await flushFinanceDeferredEffects(database);await check(false,true);
   const retained=(await attachmentDeniedReadAudits(database,actor,linkId)).find(event=>event.id===denied[0].id);
   if(retained?.witness!==denied[0].witness)throw changed();
   return {kind:'denied',error};
  }
  // FileStore.get may return a lazy stream. No object, stream, or partial byte
  // buffer escapes while audit/source/current-authority checks can still fail.
  const bytes=await result.object.arrayBuffer();
  if(defer)defer(check);else{await flushFinanceDeferredEffects(database);await check();}
  const object:StoredFile={...result.object,body:new ReadableStream<Uint8Array>({start(controller){controller.enqueue(new Uint8Array(bytes));controller.close();}}),arrayBuffer:async()=>bytes.slice(0),text:async()=>new TextDecoder().decode(bytes)};
  return {kind:'read',result:{...result,object}};
 };
 const unwrap=(outcome:Outcome)=>{if(outcome.kind==='denied')throw outcome.error;return outcome.result;};
 if(inherited){const legal=await import('@erpx/server-legal/public/attachment-read-parent');return unwrap(await execute(owner,check=>legal.deferLegalAttachmentReadCheck(owner,actor.companyId,check)));}
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'WORK_TASK_FILE_READ_TRANSACTION_REQUIRED','Private task bytes require their fresh complete owned read transaction.');
 return unwrap(await owner.transaction(async database=>{await sourceLeases(database,actor.companyId);return execute(database);}));
}
