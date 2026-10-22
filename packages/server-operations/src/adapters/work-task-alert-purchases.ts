import 'server-only';
import {randomUUID} from 'node:crypto';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {isTransactionalSqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {TaskAlertRow} from '@erpx/server-operations/domain/staff-task-alerts';
import {taskParticipantReadPolicy} from '@erpx/server-operations/adapters/project-task-record-policy';
import {workTaskSocialQueuedSourceScope} from '@erpx/server-collaboration/public/work-task-queued-read-sources';
import {workTaskSocialSourceLocks,workTaskSocialAudienceSourceLocks,workTaskSocialPurchaseSource} from '@erpx/server-collaboration/public/work-task-purchase-source';
import {workApprovalSettlementSources as approval,workTaskApprovalSourceLocks} from '@erpx/server-workflow/public/work-package-obligation-sources';
import {finalRootOwnerProjectionSources as people} from '@erpx/server-people_payroll/public/final-root-owner-projection-sources';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {withProjectTaskAlert} from '@erpx/server-projects/public/scheduled-tasks';
import {workPackageEnforcementMode,projectsPackageEnforcementMode,readPurchasedAccesses,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {purchasedCapabilityPredicate,packageClockSql} from '@erpx/server-commercial/public/purchased-capability-sql';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {projectSocialSourceReadLocks} from '@erpx/server-projects/public/social-purchase-sources';
import {socialPmsSourceReadLocks} from '@erpx/server-pms/public/social-source-facts';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';

function nativeStaff(viewer:string){return `(SELECT COUNT(*) FROM (${people.hr_people}) person JOIN (${people.hr_engagements}) engagement ON engagement.person_id=person.id AND engagement.company_id=person.company_id WHERE person.company_id=t.company_id AND person.status='active' AND person.merged_into_id IS NULL AND iam_same_person(person.user_id,${viewer}.user_id,person.company_id) AND engagement.status='active' AND engagement.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (engagement.end_date IS NULL OR engagement.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) AND (${viewer}.scope_kind='holding' OR ${viewer}.scope_kind='branch' AND ${viewer}.scope_node_id=engagement.branch_id OR ${viewer}.scope_kind='department' AND ${viewer}.scope_node_id=engagement.department_id))=1`;}
function authority(viewer:string,social='TRUE'){return `iam_assignment_active(${viewer}.assignment_id,${viewer}.now_ms) AND iam_principal_active(${viewer}.user_id,t.company_id,${viewer}.now_ms) AND EXISTS(SELECT 1 FROM iam_roles role WHERE role.id=${viewer}.role AND role.company_id IN ('*',t.company_id) AND role.status='active') AND ${nativeStaff(viewer)} AND ${scheduledFeaturePredicate('t.company_id','operations')} AND ${taskParticipantReadPolicy('t.id','t.company_id',viewer)} AND (${social})`;}
function purchased(socialRequired:string){return `${purchasedCapabilityPredicate('t.company_id','work.actions','settle-existing','work_alert_clock.now_ms')} AND (t.template_id IS NULL OR ${purchasedCapabilityPredicate('t.company_id','work.advanced-routines','settle-existing','work_alert_clock.now_ms')}) AND (NOT ${socialRequired} OR ${purchasedCapabilityPredicate('t.company_id','work.social','settle-existing','work_alert_clock.now_ms')})`;}
/** Only actual ordinary Task recipients participate; Project and every other
 * product's foundation actions retain their separate original owner. */
export async function workTaskAlertReadScope(now:number,deadline=now+86400000){
 if(await workPackageEnforcementMode()!=='enforced')return {prefix:'',where:'TRUE',args:[]};
 if(!Number.isSafeInteger(now))throw new ApiError(422,'WORK_ALERT_CLOCK_REQUIRED','A current scheduled task clock is required.');
 const social=await workTaskSocialQueuedSourceScope({deadline}),recipient=`(iam_same_person(work_alert_viewer.user_id,t.assignee_user_id,t.company_id) OR iam_same_person(work_alert_viewer.user_id,d.delegate_user_id,t.company_id) OR (t.due_at<${now} AND iam_same_person(work_alert_viewer.user_id,t.creator_user_id,t.company_id)) OR EXISTS(SELECT 1 FROM (${approval.instances}) pending_instance JOIN (${approval.steps}) pending_step ON pending_step.instance_id=pending_instance.id AND pending_step.state='pending' WHERE pending_instance.company_id=t.company_id AND pending_instance.record_id=t.id AND pending_instance.state='pending' AND iam_same_person(pending_step.approver_user_id,work_alert_viewer.user_id,t.company_id)))`;
 return {prefix:social.prefix,args:social.args,where:`(t.project_id IS NOT NULL OR EXISTS(WITH work_alert_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT 1 FROM work_alert_clock WHERE ${purchased(social.socialRequired)} AND EXISTS(SELECT 1 FROM assignments assignment CROSS JOIN LATERAL(SELECT assignment.id assignment_id,assignment.user_id,assignment.role,assignment.scope_kind,assignment.scope_node_id,work_alert_clock.now_ms) work_alert_viewer WHERE assignment.company_id=t.company_id AND ${recipient} AND ${authority('work_alert_viewer',social.where)})))`};
}
const imageSql=`SELECT jsonb_build_object('task',to_jsonb(t),'template',(SELECT to_jsonb(template) FROM routine_templates template WHERE template.id=t.template_id AND template.company_id=t.company_id),'schedule',(SELECT to_jsonb(schedule) FROM routine_schedules schedule WHERE schedule.id=t.schedule_id AND schedule.company_id=t.company_id),'delegations',(SELECT jsonb_agg(to_jsonb(delegation) ORDER BY delegation.id) FROM staff_task_delegations delegation WHERE delegation.task_id=t.id AND delegation.company_id=t.company_id),'approval',(SELECT jsonb_agg(jsonb_build_object('instance',to_jsonb(instance),'steps',(SELECT jsonb_agg(to_jsonb(step) ORDER BY step.id) FROM (${approval.steps}) step WHERE step.instance_id=instance.id),'decisions',(SELECT jsonb_agg(to_jsonb(decision) ORDER BY decision.id) FROM (${approval.decisions}) decision WHERE decision.instance_id=instance.id)) ORDER BY instance.id) FROM (${approval.instances}) instance WHERE instance.record_id=t.id AND instance.company_id=t.company_id))::text image FROM staff_tasks t WHERE t.id=? AND t.company_id=?`;
const recipientSql=(social:string)=>`SELECT assignment.id,assignment.version FROM assignments assignment CROSS JOIN LATERAL(SELECT assignment.id assignment_id,assignment.user_id,assignment.role,assignment.scope_kind,assignment.scope_node_id,${packageClockSql} now_ms) work_alert_viewer JOIN staff_tasks t ON t.company_id=assignment.company_id WHERE assignment.user_id=? AND assignment.company_id=? AND t.id=? AND ${authority('work_alert_viewer',social)} ORDER BY assignment.id`;
type Target={id:string;companyId:string;userId:string;version:number};
/** Pin a real current recipient assignment, never fabricate a role/session for
 * a system delivery. The private source lease precedes the first snapshot. */
export async function withWorkTaskAlert<Result>(owner:SqlDatabase,selected:TaskAlertRow|undefined,target:Target,run:(database:SqlDatabase)=>Promise<Result>):Promise<Result|undefined>{
 const mode=await workPackageEnforcementMode();
 if(mode!=='enforced'){if(mode==='shadow'&&!selected?.project_id)await readPurchasedAccesses(owner,mode,target.companyId,['work.actions','work.advanced-routines','work.social']);return withProjectTaskAlert(owner,selected,target,run);}
 if(selected?.project_id&&await projectsPackageEnforcementMode()==='enforced')return withProjectTaskAlert(owner,selected,target,run);
 if(!selected)return undefined;
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'WORK_TRANSACTION_REQUIRED','Work alerts require their complete owned Holding transaction.');
 try{return await owner.transaction(async database=>{
  await database.batch([companyPurchaseReadLocks(database,target.companyId)[0],database.prepare('LOCK TABLE staff_tasks,staff_task_delegations,routine_templates,routine_schedules IN SHARE ROW EXCLUSIVE MODE'),workTaskApprovalSourceLocks(database),workTaskSocialSourceLocks(database),workTaskSocialAudienceSourceLocks(database),projectSocialSourceReadLocks(database),socialPmsSourceReadLocks(database)]);await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);
  const source=await database.prepare(`SELECT t.id,t.company_id,t.project_id,t.template_id,t.version,t.title,t.due_at,t.assignee_user_id,t.creator_user_id,t.state,(SELECT sd.delegate_user_id FROM staff_task_delegations sd WHERE sd.task_id=t.id AND sd.company_id=t.company_id AND sd.status='active' AND sd.starts_at<=? AND sd.expires_at>? ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1) delegate_user_id FROM staff_tasks t WHERE t.id=? AND t.company_id=?`).bind(Date.now(),Date.now(),target.id,target.companyId).first<{id:string;company_id:string;project_id:string|null;template_id:string|null;version:number;title:string;due_at:number|null;assignee_user_id:string;creator_user_id:string;delegate_user_id:string|null;state:string}>();
  if(!source||source.version!==target.version||['completed','cancelled'].includes(source.state)||(['project_id','title','due_at','assignee_user_id','creator_user_id','delegate_user_id'] as const).some(field=>(source[field]??null)!==(selected[field]??null)))return undefined;
  const socialPolicy=await workTaskSocialQueuedSourceScope({deadline:Date.now(),target:{id:target.id,companyId:target.companyId}}),image=await database.prepare(imageSql).bind(target.id,target.companyId).first<string>('image'),social=await workTaskSocialPurchaseSource(database,target.companyId,target.id),codes=source.project_id?[]:['work.actions',...(source.template_id?['work.advanced-routines']:[]),...(social.workSocial?['work.social']:[])];
  let assignment:{id:string;version:number}|undefined;
  const fences:{key:string;boundary:NonNullable<ReturnType<typeof purchaseCommandBoundary>>}[]=[];
  if(codes.length){
   const purchased=await readPurchasedAccesses(database,mode,target.companyId,codes);if(codes.some(code=>!purchased[code].settleExisting))return undefined;
   assignment=(await database.prepare(`${socialPolicy.prefix}${recipientSql(socialPolicy.where)}`).bind(...socialPolicy.args,target.userId,target.companyId,target.id).all<{id:string;version:number}>()).results[0];if(!assignment)return undefined;
   for(const capability of codes){const key=`work-alert:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,{companyId:target.companyId,capability,purpose:'settle-existing'})!;await database.batch(boundary.before);fences.push({key,boundary});}
  }
  const result=await run(database);await flushFinanceDeferredEffects(database);
  const socialAfter=await workTaskSocialPurchaseSource(database,target.companyId,target.id);
  if(!image||image!==await database.prepare(imageSql).bind(target.id,target.companyId).first<string>('image')||social.postsImage!==socialAfter.postsImage||social.originImage!==socialAfter.originImage)throw new ApiError(409,'WORK_ALERT_SOURCE_CHANGED','The original queued Work task source changed before alert commit.');
  if(assignment&&!(await database.prepare(`${socialPolicy.prefix}${recipientSql(socialPolicy.where)}`).bind(...socialPolicy.args,target.userId,target.companyId,target.id).all<{id:string;version:number}>()).results.some(row=>row.id===assignment!.id&&row.version===assignment!.version))throw new ApiError(403,'WORK_ALERT_AUTHORITY_CHANGED','The current original Work alert recipient authority changed.');
  const results=await database.batch(fences.flatMap(({key,boundary})=>[...boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]));fences[0]?.boundary.observe?.(results);return result;
 });}catch(error){throw purchaseCommandFailure(error)??error;}
}
