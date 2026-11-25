import{visibleTask,authority}from '@erpx/server-operations/adapters/evaluation-source-policy';
import {workforceActorScopeSql,operationsViewerClockHead,OPERATIONS_TASK_SCOPE_SQL,operationsClockProbe,operationsActorScopeInput,operationsPurchasedActorScopeClockExists} from './workforce-actor-scope-source';
import {nativeOperationsReadFirst,OPERATIONS_RD_RECORD_TASK_PROJECT} from './native-operations-read';
import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';

import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {WorkItem} from '@erpx/feature-work_projection/lib/work-item';
import {appsPackageEnforcementMode,projectsPackageEnforcementMode,readPurchasedAccess,readPurchasedAccesses} from '@erpx/server-commercial/public/purchase-command-boundary';
import {purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';

import {projectTaskRecordPolicy} from '@erpx/server-operations/adapters/project-task-record-policy';
import {workTaskPurchaseAccess,workTaskReadScope} from '@erpx/server-operations/adapters/work-task-purchases';
import {projectTaskReadScope} from '@erpx/server-projects/public/task-purchases';
import {routineSourceReadScope} from '@erpx/server-operations/adapters/routine-source-read';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {workTaskSocialReadScope} from '@erpx/server-collaboration/public/work-task-read-policy';
import {nativeHandbookReads} from '@erpx/feature-collaboration/lib/native-handbook-page';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {operationsStaffTaskExists} from './operations-staff-task-exists';
import {operationsTableExists} from './identity-policy-version-guard';
import {OPERATIONS_SELECT_1_FROM} from './read-tail-r56-sources';

/** Canonical project-source history probe shared by the Work and routine
 * metric gates. Both sites rendered the exact same bytes; only the source
 * table and the caller's scope predicate vary. Binds and SQL are unchanged. */
export const projectSourceProbe = (table: string, scopeWhere: string) => `${OPERATIONS_SELECT_1_FROM(`${table} t`)} WHERE t.company_id=? AND t.project_id IS NOT NULL AND ${scopeWhere} LIMIT 1`;

/**
 * Operations-owned policy fragment for records that point at a handbook
 * article. The native handbook service is the sole article reader; this
 * predicate only confines the already-visible Work/Records/Inbox rows to the
 * current assignment and purchase scope, and is owned here until a native
 * scope-predicate port replaces it.
 */
async function handbookReadScope(actor:WorkContext){
 if(await appsPackageEnforcementMode()!=='enforced')return {where:'TRUE',args:[] as (string|number|null)[]};
 const current=currentAssignmentReadScope(actor,'workforce_actor_scope');
 return {where:`EXISTS(${operationsClockProbe("handbook_clock")} CROSS JOIN LATERAL${operationsActorScopeInput('a.company_id','?::text branch_id','?::text department_id')}workforce_actor_scope WHERE ${current.where} AND ${scheduledFeaturePredicate('a.company_id','collaboration')} AND ${purchasedCapabilityPredicate('a.company_id','work.handbook','history','handbook_clock.now_ms')})`,args:[actor.branchId,actor.departmentId,...current.args]};
}
async function handbookSourceReadScope(actor:WorkContext,surface:'records'|'staff'|'inbox'){
 if(await appsPackageEnforcementMode()!=='enforced')return {where:'TRUE',args:[] as (string|number|null)[]};
 const id=surface==='records'?'workspace_records.id':surface==='staff'?'staff_notifications.record_id':'e.source_id',company=surface==='records'?'workspace_records.company_id':surface==='staff'?'staff_notifications.company_id':'e.company_id',scope=await handbookReadScope(actor),actual=operationsTableExists('handbook_articles original_handbook',`original_handbook.id=${id}`),visible=operationsTableExists('handbook_articles a',`a.id=${id} AND a.company_id=${company} AND ${scope.where} AND ((a.archived=0 AND a.published_revision IS NOT NULL) OR ?::text='holding_owner' OR ${operationsTableExists('handbook_editors editor','editor.company_id=a.company_id AND editor.user_id=?')}))`);
 const type=surface==='staff'?'staff_notifications.record_type':'e.source_type';
 return {where:surface==='records'?`(NOT ${actual} OR ${visible})`:`(${type} NOT IN ('handbook','handbook_article') OR ${visible})`,args:[...scope.args,actor.role,actor.userId]};
}
/** Native handbook read guard for a single original source. */
async function requireHandbookWorkspaceRead(actor:WorkContext,id:string){
 if(await appsPackageEnforcementMode()==='disabled')return;
 try{await (await nativeHandbookReads(actor,actor.holdingSlug)).handbookArticle(actor,id);}
 catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))throw new ApiError(404,'ARTICLE_NOT_FOUND','This original handbook source is unavailable.');throw error;}
}

/** Fixed completed-task reference alias; actual Project tasks use their named
 * Project reader instead of acquiring ordinary Work purchases. */

/** Native identity determines Task ownership. Project foundations continue
 * through their own reader; a source label cannot grant ordinary Work access. */
export async function workSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:'records'|'staff'|'inbox'){
 const mode=await appsPackageEnforcementMode();if(mode!=='enforced'){
  if(mode==='shadow')await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'work.actions'});
  return {where:'TRUE',args:[] as unknown[]};
 }
 const id=surface==='records'?'workspace_records.id':surface==='staff'?'staff_notifications.record_id':'e.source_id',company=surface==='records'?'workspace_records.company_id':surface==='staff'?'staff_notifications.company_id':'e.company_id';
 const projectTaskExists=operationsStaffTaskExists('work_record_project',id,`work_record_project.company_id=${company} AND work_record_project.project_id IS NOT NULL`);
 const task=await visibleTask(database,actor,id,company),handbook=await handbookSourceReadScope(actor,surface),actual=operationsStaffTaskExists('work_record_task',id,'work_record_task.project_id IS NULL');
 if(surface!=='records'){
  const type=surface==='staff'?'staff_notifications.record_type':'e.source_type';
  return {where:`(${type} NOT IN ('task','staff_task','staff_task_alert') OR ${projectTaskExists} OR ${task.where}) AND ${handbook.where}`,args:[...task.args,...handbook.args]};
 }
 const current=authority(actor);
 return {where:`(NOT ${actual} OR ${task.where}) AND (module_id<>'operations' OR ${projectTaskExists} OR ${operationsPurchasedActorScopeClockExists(`${company} company_id`,'workspace_records.branch_id','workspace_records.department_id',current.where)}) AND ${handbook.where}`,args:[...task.args,...current.args,...handbook.args]};
}
export async function requireWorkWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 const mode=await appsPackageEnforcementMode();if(mode==='disabled')return;
 if(moduleId==='collaboration')await requireHandbookWorkspaceRead(actor,id);
 const task=await nativeOperationsReadFirst<{project_id:string|null}>(OPERATIONS_RD_RECORD_TASK_PROJECT,[id,actor.companyId],database);
 if(task?.project_id||!task&&moduleId!=='operations')return;
 if(mode==='shadow'){await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'work.actions'});return;}
 const source=task?await visibleTask(database,actor,'work_record_input.id','work_record_input.company_id'):null,current=authority(actor);
 const where=source?.where??operationsPurchasedActorScopeClockExists('?::text company_id','?::text branch_id','?::text department_id',current.where),args=source?.args??[actor.companyId,actor.branchId,actor.departmentId,...current.args];
 if(!await database.prepare(`SELECT ${where} allowed FROM(SELECT ?::text id,?::text company_id)work_record_input`).bind(...args,id,actor.companyId).first('allowed'))throw new ApiError(404,'WORK_SOURCE_NOT_FOUND','The original Work source is unavailable to this current assignment.');
}
export async function filterWorkTaskWork(database:SqlDatabase,actor:WorkContext,items:WorkItem[]){
 if(await appsPackageEnforcementMode()==='disabled'||!items.some(item=>item.type==='staff_task'))return items;
 const output:WorkItem[]=[];
 for(const item of items){
  if(item.type!=='staff_task'){output.push(item);continue;}
  try{
   const access=await workTaskPurchaseAccess(database,actor,item.id);if(!access){output.push(item);continue;}
   const owner=await import('@erpx/server-operations/public/staff-task-read'),task=await (await import('@erpx/server-operations/public/staff-task-read')).getTask(database,actor,item.id),state=await owner.taskWorkState(database,actor,task);
   if(state.permittedActions.some(action=>['edit','submit','start','save-progress','complete','approve','reject'].includes(action))){output.push(item);continue;}
   output.push({...item,canAct:false,actionKind:'work',queue:item.queue==='completed'?'completed':'waiting',relationships:(item.relationships??[]).filter(value=>!['assigned','approval'].includes(value)),flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined});
  }catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))continue;throw error;}
 }
 return output;
}
/** Fixed native alias t, shared by metric rows and later evidence pages. The
 * original metric cohort still determines which visible tasks are counted. */
export async function workTaskProjectionReadScope(database:SqlDatabase,actor:WorkContext){
 if(await appsPackageEnforcementMode()==='disabled'&&await projectsPackageEnforcementMode()==='disabled')return {where:'TRUE',args:[] as unknown[]};
 const work=await workTaskReadScope(database,actor),project=await projectTaskReadScope(database,actor),social=await workTaskSocialReadScope(database,actor),current=authority(actor),viewer='work_projection_viewer';
 return {where:`(${work.where}) AND (${project.where}) AND (${social.where}) AND ${operationsViewerClockHead(viewer,' ')}${viewer} CROSS JOIN LATERAL${OPERATIONS_TASK_SCOPE_SQL}workforce_actor_scope WHERE ${current.where} AND ${projectTaskRecordPolicy('t.id','t.company_id',viewer)})`,args:[...work.args,...social.args,actor.userId,actor.assignmentId,...current.args]};
}
export async function workMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await appsPackageEnforcementMode(),projectMode=await projectsPackageEnforcementMode();
 const result=(tasks:boolean,routines:boolean)=>({'my-tasks':tasks,'open-operations-exceptions':tasks,'exception-resolution-time':tasks,'operations-escalations':tasks,routines});
 if(mode==='disabled'&&projectMode==='disabled')return result(true,true);
 const work=await readPurchasedAccesses(database,mode,actor.companyId,['work.actions','work.advanced-routines']),project=await readPurchasedAccess(database,projectMode,{companyId:actor.companyId,capability:'projects.tasks'});
 const current=authority(actor),live=mode==='disabled'&&projectMode==='disabled'||!!await database.prepare(workforceActorScopeSql(current.where)).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first();
 if(!live)return result(false,false);
 const source=await workTaskProjectionReadScope(database,actor),projectTask=project.history&&!!await database.prepare(projectSourceProbe('staff_tasks',source.where)).bind(actor.companyId,...source.args).first();
 const routineScope=await routineSourceReadScope(database,actor),projectRoutine=project.history&&!!await database.prepare(projectSourceProbe('routine_templates',routineScope.where)).bind(actor.companyId,...routineScope.args).first();
 const tasks=live&&(work['work.actions'].history||projectTask),routines=live&&(work['work.actions'].history&&work['work.advanced-routines'].history||projectRoutine);
 return result(tasks,routines);
}
