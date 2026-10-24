import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {ProjectTaskSourceEntry,ProjectTaskEntryScope} from '@erpx/server-projects/domain/product-entry';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {projectsPackageEnforcementMode,readPurchasedAccess} from '@erpx/server-commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {ApiError} from '@erpx/server-runtime/public/http';
/** Role candidate discovery uses current native owner policy; it never chooses
 * a role or creates an acting WorkContext from a candidate assignment. */
async function projectTaskProductRecord(database:SqlDatabase,scope:ProjectTaskEntryScope,target:ProjectTaskSourceEntry){
 if(!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 // The original task policy imports approvals. Load its named port lazily to
 // avoid the Projects / Operations / workflow module initialization cycle.
 const {projectTaskRecordPolicy}=await import('@erpx/server-operations/public/project-task-record-policy');
 const row=await database.prepare(`WITH project_entry_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms),
 project_entry_viewer AS MATERIALIZED (SELECT g.id assignment_id,g.user_id,g.role,clock.now_ms FROM assignments g CROSS JOIN project_entry_clock clock
 WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=? AND g.version=?
 AND iam_assignment_active(g.id,clock.now_ms) AND iam_principal_active(g.user_id,g.company_id,clock.now_ms)
 AND iam_role_allows(g.role,g.company_id,'records.read')
 AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=? OR g.scope_kind='department' AND g.scope_node_id=?))
 SELECT t.title,t.state status FROM staff_tasks t JOIN staff_projects p ON p.id=t.project_id AND p.company_id=t.company_id CROSS JOIN project_entry_viewer
 WHERE t.id=? AND t.company_id=? AND ${projectTaskRecordPolicy('t.id','t.company_id','project_entry_viewer')}
 AND ${purchasedCapabilityPredicate('t.company_id','projects.wbs','history','project_entry_viewer.now_ms')}
 AND ${purchasedCapabilityPredicate('t.company_id','projects.tasks','history','project_entry_viewer.now_ms')}
 AND ${scheduledFeaturePredicate('t.company_id','projects')} AND ${scheduledFeaturePredicate('t.company_id','operations')}`)
 .bind(scope.assignmentId,scope.userId,scope.role,scope.companyId,scope.assignmentVersion,scope.branchId,scope.departmentId,target.id,scope.companyId).first<{title:string;status:string}>();
 return row;
}
export async function projectTaskProductSourceEntry(database:SqlDatabase,scope:ProjectTaskEntryScope,target:ProjectTaskSourceEntry){
 return await projectTaskProductRecord(database,scope,target)?{capability:'projects.tasks' as const,purchasePurpose:'history' as const}:null;
}
export type ProjectTaskSourceBackdrop={kind:'task';title:string;status:string};
export type ProjectTaskProductPresentationOptions={strictProduct?:true};
/** The canonical record drawer retains its original complete Task presenter.
 * This background reads the actual Task only, without a Work queue snapshot. */
export async function projectTaskSourceRecordBackdrop(actor:WorkContext,target:ProjectTaskSourceEntry,options:ProjectTaskProductPresentationOptions={}):Promise<ProjectTaskSourceBackdrop|null>{
 if(!options.strictProduct&&await projectsPackageEnforcementMode()!=='enforced')return null;
 const database=(await holdingResources(actor.holdingId)).write,work=await readPurchasedAccess(database,'enforced',{companyId:actor.companyId,capability:'work.actions'});
 if(work.history)return null;
 if(options.strictProduct){
  const version=actor.authority?.assignmentVersion??actor.iam?.queuedAssignmentVersion??await database.prepare('SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=?').bind(actor.assignmentId,actor.userId,actor.companyId,actor.role).first<number>('version');
  const source=typeof version==='number'?await projectTaskProductRecord(database,{companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:version,role:actor.role},target):null;
  if(!source)throw new ApiError(404,'PROJECT_TASK_NOT_FOUND','The Project task is unavailable in this workspace.');
  return {kind:'task',title:source.title,status:source.status};
 }
 const source=await database.prepare('SELECT project_id FROM staff_tasks WHERE id=? AND company_id=?').bind(target.id,actor.companyId).first<{project_id:string|null}>();if(!source?.project_id)return null;
 const {getTask}=await import('@erpx/server-operations/public/staff-task-read'),task=await getTask(database,actor,target.id);
 return {kind:'task',title:task.title,status:task.state};
}
