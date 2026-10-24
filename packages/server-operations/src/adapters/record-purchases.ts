import{visibleTask,authority}from '@erpx/server-operations/adapters/evaluation-source-policy';
import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';

import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {WorkItem} from '@erpx/feature-work_projection/lib/work-item';
import {workPackageEnforcementMode,projectsPackageEnforcementMode,readPurchasedAccess,readPurchasedAccesses} from '@erpx/server-commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';

import {projectTaskRecordPolicy} from '@erpx/server-operations/adapters/project-task-record-policy';
import {workTaskPurchaseAccess,workTaskReadScope} from '@erpx/server-operations/adapters/work-task-purchases';
import {projectTaskReadScope} from '@erpx/server-projects/public/task-purchases';
import {routineSourceReadScope} from '@erpx/server-operations/adapters/routine-source-read';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {workTaskSocialReadScope} from '@erpx/server-collaboration/public/work-task-read-policy';
import {handbookSourceReadScope,requireHandbookWorkspaceRead} from '@erpx/server-collaboration/public/handbook-purchases';

/** Fixed completed-task reference alias; actual Project tasks use their named
 * Project reader instead of acquiring ordinary Work purchases. */

/** Native identity determines Task ownership. Project foundations continue
 * through their own reader; a source label cannot grant ordinary Work access. */
export async function workSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:'records'|'staff'|'inbox'){
 const mode=await workPackageEnforcementMode();if(mode!=='enforced'){
  if(mode==='shadow')await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'work.actions'});
  return {where:'TRUE',args:[] as unknown[]};
 }
 const id=surface==='records'?'workspace_records.id':surface==='staff'?'staff_notifications.record_id':'e.source_id',company=surface==='records'?'workspace_records.company_id':surface==='staff'?'staff_notifications.company_id':'e.company_id';
 const task=await visibleTask(database,actor,id,company),handbook=await handbookSourceReadScope(actor,surface),actual=`EXISTS(SELECT 1 FROM staff_tasks work_record_task WHERE work_record_task.id=${id} AND work_record_task.project_id IS NULL)`;
 if(surface!=='records'){
  const type=surface==='staff'?'staff_notifications.record_type':'e.source_type';
  return {where:`(${type} NOT IN ('task','staff_task','staff_task_alert') OR EXISTS(SELECT 1 FROM staff_tasks work_record_project WHERE work_record_project.id=${id} AND work_record_project.company_id=${company} AND work_record_project.project_id IS NOT NULL) OR ${task.where}) AND ${handbook.where}`,args:[...task.args,...handbook.args]};
 }
 const current=authority(actor);
 return {where:`(NOT ${actual} OR ${task.where}) AND (module_id<>'operations' OR EXISTS(SELECT 1 FROM staff_tasks work_record_project WHERE work_record_project.id=${id} AND work_record_project.company_id=${company} AND work_record_project.project_id IS NOT NULL) OR EXISTS(SELECT 1 FROM(SELECT ${company} company_id,workspace_records.branch_id,workspace_records.department_id,${packageClockSql} now_ms)workforce_actor_scope WHERE ${current.where} AND ${purchasedCapabilityPredicate('workforce_actor_scope.company_id','work.actions','history','workforce_actor_scope.now_ms')} AND ${scheduledFeaturePredicate('workforce_actor_scope.company_id','operations')})) AND ${handbook.where}`,args:[...task.args,...current.args,...handbook.args]};
}
export async function requireWorkWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 const mode=await workPackageEnforcementMode();if(mode==='disabled')return;
 if(moduleId==='collaboration')await requireHandbookWorkspaceRead(database,actor,id);
 const task=await database.prepare('SELECT project_id FROM staff_tasks WHERE id=? AND company_id=?').bind(id,actor.companyId).first<{project_id:string|null}>();
 if(task?.project_id||!task&&moduleId!=='operations')return;
 if(mode==='shadow'){await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'work.actions'});return;}
 const source=task?await visibleTask(database,actor,'work_record_input.id','work_record_input.company_id'):null,current=authority(actor);
 const where=source?.where??`EXISTS(SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id,${packageClockSql} now_ms)workforce_actor_scope WHERE ${current.where} AND ${purchasedCapabilityPredicate('workforce_actor_scope.company_id','work.actions','history','workforce_actor_scope.now_ms')} AND ${scheduledFeaturePredicate('workforce_actor_scope.company_id','operations')})`,args=source?.args??[actor.companyId,actor.branchId,actor.departmentId,...current.args];
 if(!await database.prepare(`SELECT ${where} allowed FROM(SELECT ?::text id,?::text company_id)work_record_input`).bind(...args,id,actor.companyId).first('allowed'))throw new ApiError(404,'WORK_SOURCE_NOT_FOUND','The original Work source is unavailable to this current assignment.');
}
export async function filterWorkTaskWork(database:SqlDatabase,actor:WorkContext,items:WorkItem[]){
 if(await workPackageEnforcementMode()==='disabled'||!items.some(item=>item.type==='staff_task'))return items;
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
 if(await workPackageEnforcementMode()==='disabled'&&await projectsPackageEnforcementMode()==='disabled')return {where:'TRUE',args:[] as unknown[]};
 const work=await workTaskReadScope(database,actor),project=await projectTaskReadScope(database,actor),social=await workTaskSocialReadScope(database,actor),current=authority(actor),viewer='work_projection_viewer';
 return {where:`(${work.where}) AND (${project.where}) AND (${social.where}) AND EXISTS(WITH ${viewer} AS MATERIALIZED(SELECT ?::text user_id,?::text assignment_id,${packageClockSql} now_ms) SELECT 1 FROM ${viewer} CROSS JOIN LATERAL(SELECT t.company_id,t.branch_id,t.department_id)workforce_actor_scope WHERE ${current.where} AND ${projectTaskRecordPolicy('t.id','t.company_id',viewer)})`,args:[...work.args,...social.args,actor.userId,actor.assignmentId,...current.args]};
}
export async function workMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await workPackageEnforcementMode(),projectMode=await projectsPackageEnforcementMode();
 const result=(tasks:boolean,routines:boolean)=>({'my-tasks':tasks,'open-operations-exceptions':tasks,'exception-resolution-time':tasks,'operations-escalations':tasks,routines});
 if(mode==='disabled'&&projectMode==='disabled')return result(true,true);
 const work=await readPurchasedAccesses(database,mode,actor.companyId,['work.actions','work.advanced-routines']),project=await readPurchasedAccess(database,projectMode,{companyId:actor.companyId,capability:'projects.tasks'});
 const current=authority(actor),live=mode==='disabled'&&projectMode==='disabled'||!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first();
 if(!live)return result(false,false);
 const source=await workTaskProjectionReadScope(database,actor),projectTask=project.history&&!!await database.prepare(`SELECT 1 FROM staff_tasks t WHERE t.company_id=? AND t.project_id IS NOT NULL AND ${source.where} LIMIT 1`).bind(actor.companyId,...source.args).first();
 const routineScope=await routineSourceReadScope(database,actor),projectRoutine=project.history&&!!await database.prepare(`SELECT 1 FROM routine_templates t WHERE t.company_id=? AND t.project_id IS NOT NULL AND ${routineScope.where} LIMIT 1`).bind(actor.companyId,...routineScope.args).first();
 const tasks=live&&(work['work.actions'].history||projectTask),routines=live&&(work['work.actions'].history&&work['work.advanced-routines'].history||projectRoutine);
 return result(tasks,routines);
}
