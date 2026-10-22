import "server-only";
import {randomUUID} from "node:crypto";
import type {TaskAlertRow} from "@erpx/server-operations/public/staff-task-alerts-contract";
import {ApiError} from "@erpx/server-runtime/public/http";
import {isTransactionalSqlDatabase,type SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {projectsPackageEnforcementMode,readPurchasedAccesses,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {flushFinanceDeferredEffects} from "@erpx/server-finance/public/finance-transactions";
import {scheduledFeaturePredicate} from "@erpx/server-pms/public/hospitality-rollout";
const capabilities=['projects.wbs','projects.tasks'];
export async function projectTaskAlertReadScope(now:number){
 if(await projectsPackageEnforcementMode()!=='enforced')return 'TRUE';
 const {projectTaskRecordPolicy}=await import('@erpx/server-operations/public/project-task-record-policy');
 if(!Number.isSafeInteger(now))throw new ApiError(422,'PROJECT_ALERT_CLOCK_REQUIRED','A current scheduled task clock is required.');
 const recipient=`(project_alert_viewer.user_id=t.assignee_user_id OR project_alert_viewer.user_id=d.delegate_user_id OR (t.due_at<${now} AND project_alert_viewer.user_id IN (t.creator_user_id,project_alert_source.owner_user_id)) OR EXISTS(SELECT 1 FROM approval_instances pending_instance JOIN approval_steps pending_step ON pending_step.instance_id=pending_instance.id AND pending_step.state='pending' WHERE pending_instance.company_id=t.company_id AND pending_instance.record_type='task' AND pending_instance.record_id=t.id AND pending_instance.state='pending' AND pending_step.approver_user_id=project_alert_viewer.user_id))`;
 return `(t.project_id IS NULL OR EXISTS(WITH project_alert_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT 1 FROM staff_projects project_alert_source CROSS JOIN project_alert_clock WHERE project_alert_source.id=t.project_id AND project_alert_source.company_id=t.company_id AND ${capabilities.map(capability=>purchasedCapabilityPredicate('project_alert_source.company_id',capability,'settle-existing','project_alert_clock.now_ms')).join(' AND ')} AND ${scheduledFeaturePredicate('project_alert_source.company_id','projects')} AND EXISTS(SELECT 1 FROM assignments g CROSS JOIN LATERAL (SELECT g.id assignment_id,g.user_id,g.role,project_alert_clock.now_ms) project_alert_viewer WHERE g.company_id=t.company_id AND iam_assignment_active(g.id,project_alert_viewer.now_ms) AND iam_principal_active(g.user_id,g.company_id,project_alert_viewer.now_ms) AND iam_role_allows(g.role,g.company_id,'records.read') AND ${recipient} AND ${projectTaskRecordPolicy('t.id','t.company_id','project_alert_viewer')})))`;
}
const imageSql=`SELECT jsonb_build_object('task',to_jsonb(t),'project',(SELECT to_jsonb(p) FROM staff_projects p WHERE p.id=t.project_id AND p.company_id=t.company_id),'delegations',(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM staff_task_delegations d WHERE d.task_id=t.id AND d.company_id=t.company_id),'approval',(SELECT jsonb_agg(jsonb_build_object('instance',to_jsonb(i),'steps',(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.id) FROM approval_steps s WHERE s.instance_id=i.id)) ORDER BY i.id) FROM approval_instances i WHERE i.record_type='task' AND i.record_id=t.id AND i.company_id=t.company_id),'origin',(SELECT jsonb_agg(jsonb_build_object('origin',to_jsonb(o),'post',to_jsonb(p)) ORDER BY o.post_id) FROM social_task_origins o LEFT JOIN social_posts p ON p.id=o.post_id WHERE o.task_id=t.id))::text image FROM staff_tasks t WHERE t.id=? AND t.company_id=?`;
async function authoritySql(){const {projectTaskRecordPolicy}=await import("@erpx/server-operations/public/project-task-record-policy");return `WITH project_alert_viewer AS MATERIALIZED (SELECT g.id assignment_id,g.user_id,g.role,${packageClockSql} now_ms FROM assignments g WHERE g.id=? AND g.user_id=? AND g.company_id=? AND g.version=?) SELECT ${projectTaskRecordPolicy('t.id','t.company_id','project_alert_viewer')} allowed FROM staff_tasks t CROSS JOIN project_alert_viewer WHERE t.id=? AND t.company_id=? AND iam_assignment_active(project_alert_viewer.assignment_id,project_alert_viewer.now_ms) AND iam_principal_active(project_alert_viewer.user_id,t.company_id,project_alert_viewer.now_ms) AND iam_role_allows(project_alert_viewer.role,t.company_id,'records.read') AND ${scheduledFeaturePredicate('t.company_id','projects')} AND ${scheduledFeaturePredicate('t.company_id','operations')}`;}
/** Scheduled delivery has no caller Company role to upgrade. Pin one actual
 * current recipient assignment and the queued native task/source/version, then
 * repeat both after the original notice and every deferred owner effect. */
export async function withProjectTaskAlert<Result>(owner:SqlDatabase,selected:TaskAlertRow|undefined,target:{id:string;companyId:string;userId:string;version:number},run:(database:SqlDatabase)=>Promise<Result>):Promise<Result|undefined>{
 const mode=await projectsPackageEnforcementMode();if(mode!=='enforced'){if(mode==='shadow'&&selected?.project_id)await readPurchasedAccesses(owner,mode,target.companyId,capabilities);return run(owner);}
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'PROJECT_TRANSACTION_REQUIRED','Project task alerts require their owned Holding transaction.');
 try{return await owner.transaction(async database=>{
  await database.batch(companyPurchaseReadLocks(database,target.companyId));
  const source=await database.prepare("SELECT t.id,t.company_id,t.project_id,t.version,t.title,t.due_at,t.assignee_user_id,t.creator_user_id,t.state,(SELECT p.owner_user_id FROM staff_projects p WHERE p.id=t.project_id AND p.company_id=t.company_id) project_owner_id FROM staff_tasks t WHERE t.id=? AND t.company_id=? FOR UPDATE OF t").bind(target.id,target.companyId).first<{project_id:string|null;version:number;title:string;due_at:number|null;assignee_user_id:string;creator_user_id:string;state:string;project_owner_id:string|null}>();
  if(!source||source.version!==target.version||['completed','cancelled'].includes(source.state)||selected&&(['project_id','project_owner_id','title','due_at','assignee_user_id','creator_user_id'] as const).some(field=>(selected[field]??null)!==(source[field]??null)))return undefined;
  const before=await database.prepare(imageSql).bind(target.id,target.companyId).first<string>('image');
  const boundaries:{key:string;boundary:NonNullable<ReturnType<typeof purchaseCommandBoundary>>}[]=[];
  let assignment:{id:string;version:number}|null=null;
  if(source.project_id){
   if(!await database.prepare("SELECT id FROM staff_projects WHERE id=? AND company_id=? FOR SHARE").bind(source.project_id,target.companyId).first())return undefined;
   const access=await readPurchasedAccesses(database,mode,target.companyId,capabilities);if(capabilities.some(capability=>!access[capability].settleExisting))return undefined;
   const candidates=(await database.prepare(`SELECT id,version FROM assignments WHERE user_id=? AND company_id=? AND iam_assignment_active(id,${packageClockSql}) AND iam_principal_active(user_id,company_id,${packageClockSql}) AND iam_role_allows(role,company_id,'records.read') ORDER BY id`).bind(target.userId,target.companyId).all<{id:string;version:number}>()).results;
   for(const candidate of candidates){if(await database.prepare(await authoritySql()).bind(candidate.id,target.userId,target.companyId,candidate.version,target.id,target.companyId).first('allowed')){assignment=candidate;break;}}
   if(!assignment)return undefined;await database.prepare("SELECT id FROM assignments WHERE id=? FOR SHARE").bind(assignment.id).all();
   for(const capability of capabilities){const key=`project-alert:${randomUUID()}`,boundary=purchaseCommandBoundary(database,mode,key,{companyId:target.companyId,capability,purpose:'settle-existing'})!;await database.batch(boundary.before);boundaries.push({key,boundary});}
  }
  const result=await run(database);await flushFinanceDeferredEffects(database);
  if(!before||before!==await database.prepare(imageSql).bind(target.id,target.companyId).first<string>('image'))throw new ApiError(409,'PROJECT_ALERT_SOURCE_CHANGED','The queued Project task source changed before alert commit.');
  if(assignment&&!await database.prepare(await authoritySql()).bind(assignment.id,target.userId,target.companyId,assignment.version,target.id,target.companyId).first('allowed'))throw new ApiError(403,'PROJECT_ALERT_AUTHORITY_CHANGED','The current Project alert recipient authority changed.');
  const observations=await database.batch(boundaries.flatMap(({key,boundary})=>[...boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]));boundaries[0]?.boundary.observe?.(observations);return result;
 });}catch(error){throw purchaseCommandFailure(error)??error;}
}
