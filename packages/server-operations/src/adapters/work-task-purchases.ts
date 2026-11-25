import 'server-only';
import {operationsChildJsonByBoundId} from './task-delegation-source';
import {operationsClockProbe,operationsActorScopeInputExists} from './workforce-actor-scope-source';
import {randomUUID} from 'node:crypto';
import {nativeOperationsReadFirst,OPERATIONS_RD_WORK_TASK_SOURCE,OPERATIONS_RD_WORK_TASK_IMAGE,OPERATIONS_RD_WORK_TASK_REPLAY_RECEIPT} from './native-operations-read';
import type {StaffActor} from '@erpx/server-operations/public/staff-policy';
import {staffGuard} from '@erpx/server-operations/adapters/staff-policy';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {projectSocialSourceReadLocks} from '@erpx/server-projects/public/social-purchase-sources';
import {socialPmsSourceReadLocks} from '@erpx/server-pms/public/social-source-facts';
import {iamMutationGuards} from '@erpx/server-identity_access/public/iam-store';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';
import {workTaskApprovalSourceLocks} from '@erpx/server-workflow/public/work-package-obligation-sources';
import {workTaskSocialReadPolicy,workTaskSocialReadScope} from '@erpx/server-collaboration/public/work-task-read-policy';
import {workTaskSocialPurchasePredicate,workTaskSocialSourceLocks,workTaskSocialAudienceSourceLocks,workTaskSocialPurchaseSource} from '@erpx/server-collaboration/public/work-task-purchase-source';
import {withOperationsTaskPurchaseUnit,deferOperationsTaskPurchaseCheck,withProjectTaskPurchasedCommand} from '@erpx/server-projects/public/task-purchases';
import {appsPackageEnforcementMode,projectsPackageEnforcementMode,checkPurchasedCapability,readPurchasedAccesses,purchaseCommandBoundary,companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import type {CapabilityPurpose} from '@erpx/server-commercial/public/rules/packages';
import type {TaskWorkState} from '@erpx/server-operations/domain/staff-tasks';
import {clearWorkflowCommandAssertions} from "@erpx/server-workflow/public/command-assertion-effects";
type Command={key:string;operations:readonly string[];id:string;purpose:CapabilityPurpose;configure?:boolean;projectId?:string|null;assigneeUserId?:string;sourcePostId?:string};
type TaskSource={id:string;company_id:string;branch_id:string|null;department_id:string|null;creator_user_id:string;assignee_user_id:string;project_id:string|null;template_id:string|null;schedule_id:string|null;state:string;version:number};
const attachmentActions=new WeakMap<SqlDatabase,{id:string;userId:string}>();
export function workTaskOwnsAttachmentAction(database:SqlDatabase,actor:StaffActor,id:string){const action=attachmentActions.get(database);return action?.id===id&&action.userId===actor.userId;}
const source=(database:SqlDatabase,actor:StaffActor,id:string)=>nativeOperationsReadFirst<TaskSource>(OPERATIONS_RD_WORK_TASK_SOURCE,[id,actor.companyId],database);
function authority(actor:StaffActor){const scope=currentAssignmentReadScope(actor,'workforce_actor_scope');return {sql:operationsActorScopeInputExists(`${scope.where} AND ${scheduledFeaturePredicate('workforce_actor_scope.company_id','operations')}`),args:[actor.companyId,actor.branchId,actor.departmentId,...scope.args]};}
async function current(database:SqlDatabase,actor:StaffActor,key:string,write=false){const scope=authority(actor);if(!await database.prepare(scope.sql).bind(...scope.args).first())throw new ApiError(403,'WORK_TASK_AUTHORITY_CHANGED','Choose the current original task account and Company role.');if(write)await database.batch([assertion(database,key,'work-task-current',`EXISTS(${scope.sql})`,scope.args),...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),staffGuard(database,key,actor.companyId,actor.userId,'work-task-actor'),clearWorkflowCommandAssertions(database, key)]);}
async function capabilities(database:SqlDatabase,actor:StaffActor,task:TaskSource|null,ordinary:boolean,postId?:string){
 if(!ordinary)return [];const codes=['work.actions'];if(task?.template_id)codes.push('work.advanced-routines');if((await workTaskSocialPurchaseSource(database,actor.companyId,task?.id??'',postId)).workSocial)codes.push('work.social');return codes;
}
/** Owner-planned completed task image. The native registry owns the composed
 * literal; the caller runs it on its open unit with the unchanged `[id, company]`
 * bind, and `first('image')` becomes the first row's `image` column. */
const taskImage=(database:SqlDatabase,id:string,companyId:string)=>nativeOperationsReadFirst<{image:string}>(OPERATIONS_RD_WORK_TASK_IMAGE,[id,companyId],database).then(row=>row?.image??null);
const referencesImageSql=`SELECT jsonb_build_object('template',${operationsChildJsonByBoundId('routine_templates','t')},'schedule',${operationsChildJsonByBoundId('routine_schedules','s')})::text image`;
/** A complete existing owner unit composes both actual Project and ordinary
 * Work sources. No Work purchase is added to Project-only task foundations. */
export async function withWorkTaskPurchasedCommand<Result>(owner:SqlDatabase,actor:StaffActor,command:Command,run:(database:SqlDatabase)=>Promise<Result>){
 const mode=await appsPackageEnforcementMode();if(mode==='disabled'&&await projectsPackageEnforcementMode()!=='enforced')return withProjectTaskPurchasedCommand(owner,actor,command,run);
 return withOperationsTaskPurchaseUnit(owner,actor,async database=>{await database.batch([companyPurchaseReadLocks(database,actor.companyId)[0],database.prepare('LOCK TABLE staff_tasks,staff_task_delegations,routine_templates,routine_schedules IN SHARE ROW EXCLUSIVE MODE'),workTaskApprovalSourceLocks(database),workTaskSocialSourceLocks(database),workTaskSocialAudienceSourceLocks(database),projectSocialSourceReadLocks(database),socialPmsSourceReadLocks(database)]);await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);},async database=>{
  const before=await source(database,actor,command.id),receipt=await nativeOperationsReadFirst<{operation:string}>(OPERATIONS_RD_WORK_TASK_REPLAY_RECEIPT,[command.key,actor.userId,actor.companyId],database),replay=!!receipt&&command.operations.includes(receipt.operation),expectedProject=replay?before?.project_id??null:command.configure?command.projectId??null:before?.project_id??null,ordinary=!!before&&!before.project_id||expectedProject===null,expectedAssignee=command.configure&&!replay?command.assigneeUserId:before?.assignee_user_id;
  const codes=mode==='disabled'?[]:await capabilities(database,actor,before,ordinary,replay?undefined:command.sourcePostId),purpose=replay?'history' as const:command.purpose,key=`work-task:${randomUUID()}`,boundaries=codes.map((capability,index)=>{const id=`${key}:${index}`,scope={companyId:actor.companyId,capability,purpose};return {id,scope,boundary:purchaseCommandBoundary(database,mode,id,scope)!};});
  await current(database,actor,key,true);await requireSocialSource(database,actor,command.id);if(codes.length){for(const item of boundaries)await checkPurchasedCapability(database,mode,item.scope);await database.batch(boundaries.flatMap(item=>item.boundary.before));}
  const socialBefore=await workTaskSocialPurchaseSource(database,actor.companyId,command.id,replay?undefined:command.sourcePostId),referencesBefore=before?await database.prepare(referencesImageSql).bind(before.template_id,actor.companyId,before.schedule_id,actor.companyId).first<string>('image'):null;
  const result=await withProjectTaskPurchasedCommand(database,actor,command,async tx=>{attachmentActions.set(tx,{id:command.id,userId:actor.userId});try{return await run(tx);}finally{attachmentActions.delete(tx);}}),completedImage=await taskImage(database,command.id,actor.companyId),socialCompleted=await workTaskSocialPurchaseSource(database,actor.companyId,command.id,replay?undefined:command.sourcePostId);
  deferOperationsTaskPurchaseCheck(database,actor,async()=>{
   const after=await source(database,actor,command.id),ack=result&&typeof result==='object'?result as Record<string,unknown>:null;
   if(!after||after.project_id!==expectedProject||after.assignee_user_id!==expectedAssignee||before&&(['id','company_id','branch_id','department_id','creator_user_id','template_id','schedule_id'] as const).some(field=>before[field]!==after[field])||!before&&after.creator_user_id!==actor.userId||!replay&&ack?.id===command.id&&(after.version!==ack.version||typeof ack.state==='string'&&after.state!==ack.state)||!completedImage||completedImage!==await taskImage(database,command.id,actor.companyId))throw new ApiError(409,'WORK_TASK_SOURCE_CHANGED','The completed task source or original owner changed during deferred effects.');
   const socialAfter=await workTaskSocialPurchaseSource(database,actor.companyId,command.id,replay?undefined:command.sourcePostId);
   if(socialBefore.postsImage!==socialAfter.postsImage||socialCompleted.originImage!==socialAfter.originImage||socialCompleted.postsImage!==socialAfter.postsImage||before&&referencesBefore!==await database.prepare(referencesImageSql).bind(before.template_id,actor.companyId,before.schedule_id,actor.companyId).first<string>('image'))throw new ApiError(409,'WORK_TASK_SOURCE_CHANGED','The original routine or social source changed during the task command.');
   await current(database,actor,key,true);await requireSocialSource(database,actor,command.id);if(codes.length){const results=await database.batch(boundaries.flatMap(item=>[...item.boundary.after,clearWorkflowCommandAssertions(database, item.id)]));boundaries[0]?.boundary.observe?.(results);}
  });return result;
 });
}
async function requireSocialSource(database:SqlDatabase,actor:StaffActor,id:string){const policy=await workTaskSocialReadPolicy(database,actor,id);if(policy.where!=='TRUE'&&!await database.prepare(`SELECT 1 WHERE ${policy.where}`).bind(...policy.args).first())throw new ApiError(403,'WORK_TASK_SOURCE_UNAVAILABLE','The original conversation source is no longer available to this task account.');}
export async function workTaskPurchaseAccess(database:SqlDatabase,actor:StaffActor,id:string){const mode=await appsPackageEnforcementMode();if(mode==='disabled'&&await projectsPackageEnforcementMode()!=='enforced')return;await requireSocialSource(database,actor,id);if(mode==='disabled')return;const task=await source(database,actor,id);if(!task||task.project_id)return;await current(database,actor,`work-task-read:${randomUUID()}`);const codes=await capabilities(database,actor,task,true),all=await readPurchasedAccesses(database,mode,actor.companyId,codes);if(codes.some(code=>!all[code].history))throw new ApiError(403,'PACKAGE_REQUIRED','This ordinary Work task requires its purchased capability or retained history.');return {newWork:codes.every(code=>all[code].newWork),settleExisting:codes.every(code=>all[code].settleExisting),history:true,revision:codes.map(code=>all[code].revision).join(':')};}
export async function workTaskReadScope(database:SqlDatabase,actor:StaffActor){
 const mode=await appsPackageEnforcementMode();if(mode==='disabled'&&await projectsPackageEnforcementMode()!=='enforced')return {where:'TRUE',args:[] as (string|number|null)[]};
 const social=await workTaskSocialReadScope(database,actor);if(mode!=='disabled'){await current(database,actor,`work-task-list:${randomUUID()}`);await readPurchasedAccesses(database,mode,actor.companyId,['work.actions','work.advanced-routines','work.social']);}if(mode!=='enforced')return social;
 const live=authority(actor);return {args:[...live.args,...social.args],where:`EXISTS(${live.sql}) AND (${social.where}) AND (t.project_id IS NOT NULL OR EXISTS(${operationsClockProbe("work_task_clock")} WHERE ${purchasedCapabilityPredicate('t.company_id','work.actions','history','work_task_clock.now_ms')} AND (t.template_id IS NULL OR ${purchasedCapabilityPredicate('t.company_id','work.advanced-routines','history','work_task_clock.now_ms')}) AND (NOT ${workTaskSocialPurchasePredicate()} OR ${purchasedCapabilityPredicate('t.company_id','work.social','history','work_task_clock.now_ms')})))`};
}
export async function workTaskWorkState(database:SqlDatabase,actor:StaffActor,id:string,state:TaskWorkState){const access=await workTaskPurchaseAccess(database,actor,id);if(!access)return state;const permittedActions=state.permittedActions.filter(action=>['edit','revise','submit'].includes(action)?access.newWork:access.settleExisting);return {...state,permittedActions,blockedReason:state.blockedReason??(permittedActions.length===0&&state.permittedActions.length>0?'This retained Work task is available for history.':null)};}
