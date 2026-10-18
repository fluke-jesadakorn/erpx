import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '../../identity_access/public/authorization';
import {currentAssignmentReadScope} from '../../identity_access/public/record-source-authority';
import {lockQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {iamMutationGuards} from '../../identity_access/public/iam-store';
import {assertion,workContextGuards} from '../../workflow/public/commands';
import {workTaskApprovalSourceLocks} from '../../workflow/public/work-package-obligation-sources';
import {workTaskSocialFileSourceRows,workTaskSocialSourceLocks,workTaskSocialAudienceSourceLocks} from '../../collaboration/public/work-task-purchase-source';
import {workTaskSocialReadPolicy} from '../../collaboration/public/work-task-read-policy';
import {projectSocialSourceReadLocks} from '../../projects/public/social-purchase-sources';
import {socialPmsSourceReadLocks} from '../../pms/public/social-source-facts';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {workPackageEnforcementMode,companyPurchaseReadLocks,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults} from '../../commercial/public/purchase-command-boundary';
import type {CapabilityPurpose} from '../../commercial/public/rules/packages';
import {financeDeferredEffectStatements} from '../../finance/public/finance-transactions';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import type {AttachmentOperation} from '../../files/public/attachments/types';
import {ApiError} from '../../runtime/public/http';
import {staffGuard} from './staff-policy';
import {projectTaskRecordPolicy} from './project-task-record-policy';
import {workTaskOwnsAttachmentAction} from './work-task-purchases';

export type WorkTaskAttachmentPurchase={companyId:string;targetId:string;actor:WorkContext;purpose:CapabilityPurpose;capabilities:string[];witness:string;ownedAction:boolean;writer:'creator'|'assignee'|'delegate'};
function source(ownedAction:boolean){return `SELECT t.id,t.company_id,t.branch_id,t.department_id,t.project_id,t.state,t.creator_user_id,t.assignee_user_id,t.template_id,social.work_social,
 jsonb_build_object('task',to_jsonb(t)-'version'-'updated_at'${ownedAction?"-'state'-'completion_note'-'blocked_reason'":''},
 'template',(SELECT to_jsonb(template) FROM routine_templates template WHERE template.id=t.template_id AND template.company_id=t.company_id),
 'schedule',(SELECT to_jsonb(schedule) FROM routine_schedules schedule WHERE schedule.id=t.schedule_id AND schedule.company_id=t.company_id),
 'delegations',(SELECT jsonb_agg(to_jsonb(delegation) ORDER BY delegation.id) FROM staff_task_delegations delegation WHERE delegation.task_id=t.id AND delegation.company_id=t.company_id),
 'social',social.image)::text witness
 FROM staff_tasks t LEFT JOIN (${workTaskSocialFileSourceRows}) social ON social.id=t.id AND social.company_id=t.company_id WHERE t.id=? AND t.company_id=?`;}
const fileUnits=new WeakSet<SqlDatabase>();
type Source={id:string;company_id:string;branch_id:string|null;department_id:string|null;project_id:string|null;state:string;creator_user_id:string;assignee_user_id:string;template_id:string|null;work_social:boolean;witness:string};
export async function workTaskAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,type:string,id:string,operation?:AttachmentOperation):Promise<WorkTaskAttachmentPurchase|null>{
 const mode=await workPackageEnforcementMode();if(type!=='staff_task'||mode==='disabled')return null;
 const ownedAction=workTaskOwnsAttachmentAction(database,actor,id),row=await database.prepare(source(ownedAction)).bind(id,actor.companyId).first<Source>();
 if(!row)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original task evidence owner is unavailable.');
 // The existing Project owner separately licenses its actual foundation.
 if(row.project_id)return null;
 if(mode==='enforced'&&operation!=='read'&&!isTransactionalSqlDatabase(database)&&!fileUnits.has(database)&&!ownedAction)throw new ApiError(503,'WORK_TASK_FILE_TRANSACTION_REQUIRED','Task evidence requires its complete owned Files or task transaction.');
 const creator=row.state==='draft',assigned=await database.prepare('SELECT iam_same_person(?,?,?) assigned').bind(row.assignee_user_id,actor.userId,actor.companyId).first<boolean>('assigned');
 return {companyId:actor.companyId,targetId:id,actor,purpose:creator?'new-work':'settle-existing',capabilities:['work.actions',...(row.template_id?['work.advanced-routines']:[]),...(row.work_social?['work.social']:[])],witness:row.witness,ownedAction,writer:creator?'creator':assigned?'assignee':'delegate'};
}
export async function authorizeWorkTaskAttachmentPurchase(database:SqlDatabase,actor:WorkContext,type:string,id:string,operation:AttachmentOperation){
 const scope=await workTaskAttachmentPurchaseScope(database,actor,type,id,operation);if(!scope)return;
 for(const capability of scope.capabilities)await checkPurchasedCapability(database,await workPackageEnforcementMode(),{companyId:scope.companyId,capability,purpose:operation==='read'?'history':scope.purpose});
}
/** A file command leases its actual native task, routine/social source and
 * current original IAM/HR before its first transactional SELECT. */
export async function workTaskAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly WorkTaskAttachmentPurchase[]){
 if(!scopes.length||await workPackageEnforcementMode()==='disabled')return [];
 const locks=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(company=>companyPurchaseReadLocks(database,company));
 await database.batch([...locks.map(items=>items[0]),database.prepare('LOCK TABLE staff_tasks,staff_task_delegations,routine_templates,routine_schedules IN SHARE ROW EXCLUSIVE MODE'),workTaskApprovalSourceLocks(database),workTaskSocialSourceLocks(database),workTaskSocialAudienceSourceLocks(database),projectSocialSourceReadLocks(database),socialPmsSourceReadLocks(database)]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);fileUnits.add(database);
 return locks.flatMap(items=>items.slice(1));
}
export async function workTaskAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly WorkTaskAttachmentPurchase[]){
 const mode=await workPackageEnforcementMode();if(!scopes.length||mode==='disabled')return;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.targetId}:${scope.purpose}:${scope.actor.userId}`,scope])).values()],boundaries=unique.flatMap(scope=>scope.capabilities.map(capability=>{const key=`work-task-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,{companyId:scope.companyId,capability,purpose:scope.purpose})!};}));
 const social=await Promise.all(unique.map(scope=>workTaskSocialReadPolicy(database,scope.actor,scope.targetId)));
 const guards=(phase:string)=>unique.flatMap((scope,index)=>{
  const actor=scope.actor,key=`work-task-file-${phase}:${randomUUID()}`,viewer='work_task_file_actor',current=currentAssignmentReadScope(actor,'workforce_actor_scope');
  const writer=scope.writer==='creator'?`iam_same_person(work_task_file_source.creator_user_id,${viewer}.user_id,work_task_file_source.company_id)`:scope.writer==='assignee'?`iam_same_person(work_task_file_source.assignee_user_id,${viewer}.user_id,work_task_file_source.company_id)`:`EXISTS(SELECT 1 FROM staff_task_delegations delegation WHERE delegation.task_id=work_task_file_source.id AND delegation.company_id=work_task_file_source.company_id AND delegation.status='active' AND delegation.starts_at<=${viewer}.now_ms AND delegation.expires_at>${viewer}.now_ms AND iam_same_person(delegation.delegate_user_id,${viewer}.user_id,delegation.company_id))`;
  return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),staffGuard(database,key,actor.companyId,actor.userId,'work-task-file-staff'),assertion(database,key,'work-task-file-source',`EXISTS(WITH ${viewer} AS MATERIALIZED(SELECT ?::text user_id,?::text assignment_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms)
 SELECT 1 FROM (${source(scope.ownedAction)}) work_task_file_source CROSS JOIN ${viewer} WHERE work_task_file_source.project_id IS NULL AND witness=? AND ${writer}
 AND ${projectTaskRecordPolicy('work_task_file_source.id','work_task_file_source.company_id',viewer)}
 AND EXISTS(SELECT 1 FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) workforce_actor_scope WHERE ${current.where}))`,[actor.userId,actor.assignmentId,scope.targetId,scope.companyId,scope.witness,actor.companyId,actor.branchId,actor.departmentId,...current.args]),assertion(database,key,'work-task-file-social-source',social[index].where,social[index].args),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)];
 });
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards('before')],after:[...financeDeferredEffectStatements(database),...guards('after'),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key))],observe:mode==='shadow'?observePurchaseResults:undefined};
}
export function workTaskAttachmentFailure(error:unknown){let cause=error;for(let depth=0;depth<5&&cause&&typeof cause==='object';depth++){const item=cause as {code?:string;detail?:string;cause?:unknown};if(item.code==='23514'&&(item.detail?.includes('work-task-file-before:')||item.detail?.includes('work-task-file-after:')))return new ApiError(409,'WORK_TASK_ATTACHMENT_CHANGED','The original task evidence, current assignment or purchased source changed.');cause=item.cause;}return null;}
