import 'server-only';
import {operationsClockProbe} from './workforce-actor-scope-source';
import {randomUUID} from 'node:crypto';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {isTransactionalSqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import type {TaskAlertRow} from '@erpx/operations-server/domain/staff-task-alerts';
import {taskParticipantReadPolicy} from '@erpx/operations-server/adapters/project-task-record-policy';
import {workTaskSocialQueuedSourceScope} from '@erpx/collaboration-server/public/work-task-queued-read-sources';
import {workTaskSocialSourceLocks,workTaskSocialAudienceSourceLocks,workTaskSocialPurchaseSource} from '@erpx/collaboration-server/public/work-task-purchase-source';
import {workApprovalSettlementSources as approval,workTaskApprovalSourceLocks} from '@erpx/workflow-server/public/work-package-obligation-sources';
import {finalRootOwnerProjectionSources as people} from '@erpx/people-payroll-server/public/final-root-owner-projection-sources';
import {lockQueuedCommandAuthority} from '@erpx/identity-access-server/public/queued-command-authority';
import {withProjectTaskAlert} from '@erpx/projects-server/public/scheduled-tasks';
import {appsPackageEnforcementMode,projectsPackageEnforcementMode,readPurchasedAccesses,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/commercial-server/public/purchase-command-boundary';
import {purchasedCapabilityPredicate,packageClockSql} from '@erpx/commercial-server/public/purchased-capability-sql';
import { scheduledFeaturePredicate } from "@erpx/pms-server/public/feature-availability";
import {lockScheduledFeatureSources} from '@erpx/pms-server/public/scheduled-feature-source-locks';
import {projectSocialSourceReadLocks} from '@erpx/projects-server/public/social-purchase-sources';
import {socialPmsSourceReadLocks} from '@erpx/pms-server/public/social-source-facts';
import {flushFinanceDeferredEffects} from '@erpx/finance-server/public/finance-transactions';
import {clearWorkflowCommandAssertions} from "@erpx/workflow-server/public/command-assertion-effects";
import {nativeOperationsReadFirst,OPERATIONS_RD_WORK_TASK_ALERT_SOURCE,OPERATIONS_RD_WORK_TASK_ALERT_IMAGE} from './native-operations-read';
import {operationsJoinSourceExists} from './join-source-exists';
import {operationsTableExists} from './identity-policy-version-guard';

/** Canonical Work-alert recipient viewer column projection, shared by the read
 * scope and the recipient lookup so the two fragments cannot drift. The clock
 * suffix stays local to each caller. Binds unchanged. */
const workAlertViewerColumns = `CROSS JOIN LATERAL(SELECT assignment.id assignment_id,assignment.user_id,assignment.role,assignment.scope_kind,assignment.scope_node_id,`;

function nativeStaff(viewer:string){return `(SELECT COUNT(*) FROM (${people.hr_people}) person JOIN (${people.hr_engagements}) engagement ON engagement.person_id=person.id AND engagement.company_id=person.company_id WHERE person.company_id=t.company_id AND person.status='active' AND person.merged_into_id IS NULL AND iam_same_person(person.user_id,${viewer}.user_id,person.company_id) AND engagement.status='active' AND engagement.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (engagement.end_date IS NULL OR engagement.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) AND (${viewer}.scope_kind='holding' OR ${viewer}.scope_kind='branch' AND ${viewer}.scope_node_id=engagement.branch_id OR ${viewer}.scope_kind='department' AND ${viewer}.scope_node_id=engagement.department_id))=1`;}
function authority(viewer:string,social='TRUE'){return `iam_assignment_active(${viewer}.assignment_id,${viewer}.now_ms) AND iam_principal_active(${viewer}.user_id,t.company_id,${viewer}.now_ms) AND ${operationsTableExists('iam_roles role',`role.id=${viewer}.role AND role.company_id IN ('*',t.company_id) AND role.status='active'`)} AND ${nativeStaff(viewer)} AND ${scheduledFeaturePredicate('t.company_id','operations')} AND ${taskParticipantReadPolicy('t.id','t.company_id',viewer)} AND (${social})`;}
function purchased(socialRequired:string){return `${purchasedCapabilityPredicate('t.company_id','work.actions','settle-existing','work_alert_clock.now_ms')} AND (t.template_id IS NULL OR ${purchasedCapabilityPredicate('t.company_id','work.advanced-routines','settle-existing','work_alert_clock.now_ms')}) AND (NOT ${socialRequired} OR ${purchasedCapabilityPredicate('t.company_id','work.social','settle-existing','work_alert_clock.now_ms')})`;}
/** Only actual ordinary Task recipients participate; Project and every other
 * product's foundation actions retain their separate original owner. */
export async function workTaskAlertReadScope(now:number,deadline=now+86400000){
 if(await appsPackageEnforcementMode()!=='enforced')return {prefix:'',where:'TRUE',args:[]};
 if(!Number.isSafeInteger(now))throw new ApiError(422,'WORK_ALERT_CLOCK_REQUIRED','A current scheduled task clock is required.');
 const social=await workTaskSocialQueuedSourceScope({deadline}),recipient=`(iam_same_person(work_alert_viewer.user_id,t.assignee_user_id,t.company_id) OR iam_same_person(work_alert_viewer.user_id,d.delegate_user_id,t.company_id) OR (t.due_at<${now} AND iam_same_person(work_alert_viewer.user_id,t.creator_user_id,t.company_id)) OR ${operationsJoinSourceExists(approval.instances,'pending_instance',approval.steps,'pending_step',`pending_step.instance_id=pending_instance.id AND pending_step.state='pending'`,`pending_instance.company_id=t.company_id AND pending_instance.record_id=t.id AND pending_instance.state='pending' AND iam_same_person(pending_step.approver_user_id,work_alert_viewer.user_id,t.company_id)`)})`;
 return {prefix:social.prefix,args:social.args,where:`(t.project_id IS NOT NULL OR EXISTS(${operationsClockProbe("work_alert_clock")} WHERE ${purchased(social.socialRequired)} AND EXISTS(SELECT 1 FROM assignments assignment ${workAlertViewerColumns}work_alert_clock.now_ms) work_alert_viewer WHERE assignment.company_id=t.company_id AND ${recipient} AND ${authority('work_alert_viewer',social.where)})))`};
}
/** Owner-planned queued task image. The native registry owns the composed
 * literal; the caller runs it on its open unit with the unchanged `[id, company]`
 * bind, and `first('image')` becomes the first row's `image` column. */
const taskImage=(database:SqlDatabase,id:string,companyId:string)=>nativeOperationsReadFirst<{image:string}>(OPERATIONS_RD_WORK_TASK_ALERT_IMAGE,[id,companyId],database).then(row=>row?.image??null);
const recipientSql=(social:string)=>`SELECT assignment.id,assignment.version FROM assignments assignment ${workAlertViewerColumns}${packageClockSql} now_ms) work_alert_viewer JOIN staff_tasks t ON t.company_id=assignment.company_id WHERE assignment.user_id=? AND assignment.company_id=? AND t.id=? AND ${authority('work_alert_viewer',social)} ORDER BY assignment.id`;
type Target={id:string;companyId:string;userId:string;version:number};
/** Pin a real current recipient assignment, never fabricate a role/session for
 * a system delivery. The private source lease precedes the first snapshot. */
export async function withWorkTaskAlert<Result>(owner:SqlDatabase,selected:TaskAlertRow|undefined,target:Target,run:(database:SqlDatabase)=>Promise<Result>):Promise<Result|undefined>{
 const mode=await appsPackageEnforcementMode();
 if(mode!=='enforced'){if(mode==='shadow'&&!selected?.project_id)await readPurchasedAccesses(owner,mode,target.companyId,['work.actions','work.advanced-routines','work.social']);return withProjectTaskAlert(owner,selected,target,run);}
 if(selected?.project_id&&await projectsPackageEnforcementMode()==='enforced')return withProjectTaskAlert(owner,selected,target,run);
 if(!selected)return undefined;
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'WORK_TRANSACTION_REQUIRED','Work alerts require their complete owned Holding transaction.');
 try{return await owner.transaction(async database=>{
  await database.batch([companyPurchaseReadLocks(database,target.companyId)[0],database.prepare('LOCK TABLE staff_tasks,staff_task_delegations,routine_templates,routine_schedules IN SHARE ROW EXCLUSIVE MODE'),workTaskApprovalSourceLocks(database),workTaskSocialSourceLocks(database),workTaskSocialAudienceSourceLocks(database),projectSocialSourceReadLocks(database),socialPmsSourceReadLocks(database)]);await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);
  const source=await nativeOperationsReadFirst<{id:string;company_id:string;project_id:string|null;template_id:string|null;version:number;title:string;due_at:number|null;assignee_user_id:string;creator_user_id:string;delegate_user_id:string|null;state:string}>(OPERATIONS_RD_WORK_TASK_ALERT_SOURCE,[Date.now(),Date.now(),target.id,target.companyId],database);
  if(!source||source.version!==target.version||['completed','cancelled'].includes(source.state)||(['project_id','title','due_at','assignee_user_id','creator_user_id','delegate_user_id'] as const).some(field=>(source[field]??null)!==(selected[field]??null)))return undefined;
  const socialPolicy=await workTaskSocialQueuedSourceScope({deadline:Date.now(),target:{id:target.id,companyId:target.companyId}}),image=await taskImage(database,target.id,target.companyId),social=await workTaskSocialPurchaseSource(database,target.companyId,target.id),codes=source.project_id?[]:['work.actions',...(source.template_id?['work.advanced-routines']:[]),...(social.workSocial?['work.social']:[])];
  let assignment:{id:string;version:number}|undefined;
  const fences:{key:string;boundary:NonNullable<ReturnType<typeof purchaseCommandBoundary>>}[]=[];
  if(codes.length){
   const purchased=await readPurchasedAccesses(database,mode,target.companyId,codes);if(codes.some(code=>!purchased[code].settleExisting))return undefined;
   assignment=(await database.prepare(`${socialPolicy.prefix}${recipientSql(socialPolicy.where)}`).bind(...socialPolicy.args,target.userId,target.companyId,target.id).all<{id:string;version:number}>()).results[0];if(!assignment)return undefined;
   for(const capability of codes){const key=`work-alert:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,{companyId:target.companyId,capability,purpose:'settle-existing'})!;await database.batch(boundary.before);fences.push({key,boundary});}
  }
  const result=await run(database);await flushFinanceDeferredEffects(database);
  const socialAfter=await workTaskSocialPurchaseSource(database,target.companyId,target.id);
  if(!image||image!==await taskImage(database,target.id,target.companyId)||social.postsImage!==socialAfter.postsImage||social.originImage!==socialAfter.originImage)throw new ApiError(409,'WORK_ALERT_SOURCE_CHANGED','The original queued Work task source changed before alert commit.');
  if(assignment&&!(await database.prepare(`${socialPolicy.prefix}${recipientSql(socialPolicy.where)}`).bind(...socialPolicy.args,target.userId,target.companyId,target.id).all<{id:string;version:number}>()).results.some(row=>row.id===assignment!.id&&row.version===assignment!.version))throw new ApiError(403,'WORK_ALERT_AUTHORITY_CHANGED','The current original Work alert recipient authority changed.');
  const results=await database.batch(fences.flatMap(({key,boundary})=>[...boundary.after,clearWorkflowCommandAssertions(database, key)]));fences[0]?.boundary.observe?.(results);return result;
 });}catch(error){throw purchaseCommandFailure(error)??error;}
}
