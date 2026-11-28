import 'server-only';
import {operationsActiveDelegationSql} from './task-delegation-source';
import {operationsActorScopeInputExists,operationsViewerClockHead} from './workforce-actor-scope-source';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {currentAssignmentReadScope} from '@erpx/identity-access-server/public/record-source-authority';
import {lockQueuedCommandAuthority} from '@erpx/identity-access-server/public/queued-command-authority';
import {iamMutationGuards} from '@erpx/identity-access-server/public/iam-store';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {workContextGuards} from '@erpx/workflow-server/public/command-guards';
import {workTaskApprovalSourceLocks} from '@erpx/workflow-server/public/work-package-obligation-sources';
import {workTaskSocialSourceLocks,workTaskSocialAudienceSourceLocks} from '@erpx/collaboration-server/public/work-task-purchase-source';
import {workTaskSocialReadPolicy} from '@erpx/collaboration-server/public/work-task-read-policy';
import {projectSocialSourceReadLocks} from '@erpx/projects-server/public/social-purchase-sources';
import {socialPmsSourceReadLocks} from '@erpx/pms-server/public/social-source-facts';
import {lockScheduledFeatureSources} from '@erpx/pms-server/public/scheduled-feature-source-locks';
import {appsPackageEnforcementMode,companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults} from '@erpx/commercial-server/public/purchase-command-boundary';
import type {CapabilityPurpose} from '@erpx/commercial-server/public/rules/packages';
import {financeDeferredEffectStatements} from '@erpx/finance-server/public/finance-transactions';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {AttachmentOperation} from '@erpx/files-server/public/attachments/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {staffGuard} from '@erpx/operations-server/adapters/staff-policy';
import {projectTaskRecordPolicy} from '@erpx/operations-server/adapters/project-task-record-policy';
import {workTaskOwnsAttachmentAction} from '@erpx/operations-server/adapters/work-task-purchases';
import {clearWorkflowCommandAssertions} from "@erpx/workflow-server/public/command-assertion-effects";
import {
  nativeOperationsReadFirst,
  nativeOperationsReadSql,
  OPERATIONS_RD_TASK_ATTACHMENT_ASSIGNEE,
  OPERATIONS_RD_TASK_ATTACHMENT_PURCHASE_WITNESS,
  OPERATIONS_RD_TASK_ATTACHMENT_PURCHASE_WITNESS_OWNED,
} from './native-operations-read';

export type WorkTaskAttachmentPurchase={companyId:string;targetId:string;actor:WorkContext;purpose:CapabilityPurpose;capabilities:string[];witness:string;ownedAction:boolean;writer:'creator'|'assignee'|'delegate'};
/** The purchase witness is owner-planned: the two byte variants (ordinary and
 *  owned-action reduction) live in the native read registry and execute on the
 *  caller's open unit with the unchanged `[id, company_id]` bind. */
type WitnessSource={id:string;company_id:string;branch_id:string|null;department_id:string|null;project_id:string|null;state:string;creator_user_id:string;assignee_user_id:string;template_id:string|null;work_social:boolean;witness:string};
const witness=(database:SqlDatabase,ownedAction:boolean,id:string,companyId:string)=>nativeOperationsReadFirst<WitnessSource>(
  ownedAction?OPERATIONS_RD_TASK_ATTACHMENT_PURCHASE_WITNESS_OWNED:OPERATIONS_RD_TASK_ATTACHMENT_PURCHASE_WITNESS,
  [id,companyId],
  database,
);
const fileUnits=new WeakSet<SqlDatabase>();
export async function workTaskAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,type:string,id:string,operation?:AttachmentOperation):Promise<WorkTaskAttachmentPurchase|null>{
 const mode=await appsPackageEnforcementMode();if(type!=='staff_task'||mode==='disabled')return null;
 const ownedAction=workTaskOwnsAttachmentAction(database,actor,id),row=await witness(database,ownedAction,id,actor.companyId);
 if(!row)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original task evidence owner is unavailable.');
 // The existing Project owner separately licenses its actual foundation.
 if(row.project_id)return null;
 if(mode==='enforced'&&operation!=='read'&&!isTransactionalSqlDatabase(database)&&!fileUnits.has(database)&&!ownedAction)throw new ApiError(503,'WORK_TASK_FILE_TRANSACTION_REQUIRED','Task evidence requires its complete owned Files or task transaction.');
 const creator=row.state==='draft',assignee=await nativeOperationsReadFirst<{assigned:boolean}>(OPERATIONS_RD_TASK_ATTACHMENT_ASSIGNEE,[row.assignee_user_id,actor.userId,actor.companyId],database),assigned=!!assignee?.assigned;
 return {companyId:actor.companyId,targetId:id,actor,purpose:creator?'new-work':'settle-existing',capabilities:['work.actions',...(row.template_id?['work.advanced-routines']:[]),...(row.work_social?['work.social']:[])],witness:row.witness,ownedAction,writer:creator?'creator':assigned?'assignee':'delegate'};
}
export async function authorizeWorkTaskAttachmentPurchase(database:SqlDatabase,actor:WorkContext,type:string,id:string,operation:AttachmentOperation){
 const scope=await workTaskAttachmentPurchaseScope(database,actor,type,id,operation);if(!scope)return;
 for(const capability of scope.capabilities)await checkPurchasedCapability(database,await appsPackageEnforcementMode(),{companyId:scope.companyId,capability,purpose:operation==='read'?'history':scope.purpose});
}
/** A file command leases its actual native task, routine/social source and
 * current original IAM/HR before its first transactional SELECT. */
export async function workTaskAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly WorkTaskAttachmentPurchase[]){
 if(!scopes.length||await appsPackageEnforcementMode()==='disabled')return [];
 const locks=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(company=>companyPurchaseReadLocks(database,company));
 await database.batch([...locks.map(items=>items[0]),database.prepare('LOCK TABLE staff_tasks,staff_task_delegations,routine_templates,routine_schedules IN SHARE ROW EXCLUSIVE MODE'),workTaskApprovalSourceLocks(database),workTaskSocialSourceLocks(database),workTaskSocialAudienceSourceLocks(database),projectSocialSourceReadLocks(database),socialPmsSourceReadLocks(database)]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);fileUnits.add(database);
 return locks.flatMap(items=>items.slice(1));
}
export async function workTaskAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly WorkTaskAttachmentPurchase[]){
 const mode=await appsPackageEnforcementMode();if(!scopes.length||mode==='disabled')return;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.targetId}:${scope.purpose}:${scope.actor.userId}`,scope])).values()],boundaries=unique.flatMap(scope=>scope.capabilities.map(capability=>{const key=`work-task-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,{companyId:scope.companyId,capability,purpose:scope.purpose})!};}));
 const social=await Promise.all(unique.map(scope=>workTaskSocialReadPolicy(database,scope.actor,scope.targetId)));
 const planSources=await Promise.all(unique.map(scope=>nativeOperationsReadSql(scope.ownedAction?OPERATIONS_RD_TASK_ATTACHMENT_PURCHASE_WITNESS_OWNED:OPERATIONS_RD_TASK_ATTACHMENT_PURCHASE_WITNESS,[scope.targetId,scope.companyId])));
 const guards=(phase:string)=>unique.flatMap((scope,index)=>{
  const actor=scope.actor,key=`work-task-file-${phase}:${randomUUID()}`,viewer='work_task_file_actor',current=currentAssignmentReadScope(actor,'workforce_actor_scope');
  const writer=scope.writer==='creator'?`iam_same_person(work_task_file_source.creator_user_id,${viewer}.user_id,work_task_file_source.company_id)`:scope.writer==='assignee'?`iam_same_person(work_task_file_source.assignee_user_id,${viewer}.user_id,work_task_file_source.company_id)`:`EXISTS(${operationsActiveDelegationSql('1','delegation','work_task_file_source.id','work_task_file_source.company_id',`${viewer}.now_ms`,` AND iam_same_person(delegation.delegate_user_id,${viewer}.user_id,delegation.company_id)`)})`;
  return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),staffGuard(database,key,actor.companyId,actor.userId,'work-task-file-staff'),assertion(database,key,'work-task-file-source',`${operationsViewerClockHead(viewer)}(${planSources[index].sql}) work_task_file_source CROSS JOIN ${viewer} WHERE work_task_file_source.project_id IS NULL AND witness=? AND ${writer}
 AND ${projectTaskRecordPolicy('work_task_file_source.id','work_task_file_source.company_id',viewer)}
 AND EXISTS(${operationsActorScopeInputExists(current.where)}))`,[actor.userId,actor.assignmentId,...(planSources[index].values as (string|number|null)[]),scope.witness,actor.companyId,actor.branchId,actor.departmentId,...current.args]),assertion(database,key,'work-task-file-social-source',social[index].where,social[index].args),clearWorkflowCommandAssertions(database, key)];
 });
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards('before')],after:[...financeDeferredEffectStatements(database),...guards('after'),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>clearWorkflowCommandAssertions(database, item.key))],observe:mode==='shadow'?observePurchaseResults:undefined};
}
export function workTaskAttachmentFailure(error:unknown){let cause=error;for(let depth=0;depth<5&&cause&&typeof cause==='object';depth++){const item=cause as {code?:string;detail?:string;cause?:unknown};if(item.code==='23514'&&(item.detail?.includes('work-task-file-before:')||item.detail?.includes('work-task-file-after:')))return new ApiError(409,'WORK_TASK_ATTACHMENT_CHANGED','The original task evidence, current assignment or purchased source changed.');cause=item.cause;}return null;}
