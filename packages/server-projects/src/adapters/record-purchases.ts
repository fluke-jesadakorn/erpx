import "server-only";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkItem} from "@erpx/feature-work_projection/lib/work-item";
import {ApiError} from "@erpx/server-runtime/public/http";
import {projectsPackageEnforcementMode,readPurchasedAccess} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {projectTaskRecordPolicy} from "@erpx/server-operations/public/project-task-record-policy";
import {projectPurchaseControls} from "@erpx/server-projects/adapters/purchases";
import {projectTaskPurchaseAccess} from "@erpx/server-projects/adapters/task-purchases";

const nativeSources=`SELECT id,id project_id,company_id,'project' kind,'projects.wbs' capability,NULL::text source_user_id FROM staff_projects
 UNION ALL SELECT id,project_id,company_id,'milestone','projects.wbs',NULL::text FROM staff_project_milestones
 UNION ALL SELECT id,project_id,company_id,'work_log','projects.time',user_id FROM staff_project_work_logs
 UNION ALL SELECT id,project_id,company_id,'allocation','projects.costs',NULL::text FROM staff_project_cost_allocations
 UNION ALL SELECT id,project_id,company_id,'handoff','projects.handover',NULL::text FROM staff_project_handoffs
 UNION ALL SELECT id,project_id,company_id,'task','projects.tasks',NULL::text FROM staff_tasks WHERE project_id IS NOT NULL`;
const sourceKinds="('project','staff_project','project_milestone','project_work_log','project_actual_time','project_cost_allocation','project_handoff')";
function visibleSource(actor:WorkContext,id:string,company:string,kind?:'project'|'task'){
 const viewer='project_source_viewer',taskPolicy=projectTaskRecordPolicy('native.id','native.company_id',viewer);
 const current=`EXISTS(SELECT 1 FROM assignments g WHERE g.id=${viewer}.assignment_id AND g.user_id=${viewer}.user_id AND g.role=${viewer}.role AND g.company_id=native.company_id
 AND iam_assignment_active(g.id,${viewer}.now_ms) AND iam_principal_active(g.user_id,g.company_id,${viewer}.now_ms) AND iam_role_allows(g.role,g.company_id,'records.read')
 AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${viewer}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${viewer}.department_id))`;
 const member=`(p.owner_user_id=${viewer}.user_id OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=${viewer}.user_id))`;
 const purchase=['projects.wbs','projects.tasks','projects.time','projects.costs','projects.handover'].map(code=>`(native.capability='${code}' AND ${purchasedCapabilityPredicate('native.company_id',code,'history',`${viewer}.now_ms`)})`).join(' OR ');
 return {sql:`EXISTS(WITH ${viewer} AS MATERIALIZED (SELECT ?::text user_id,?::text assignment_id,?::text role,?::text branch_id,?::text department_id,${packageClockSql} now_ms)
 SELECT 1 FROM (${nativeSources}) native JOIN staff_projects p ON p.id=native.project_id AND p.company_id=native.company_id CROSS JOIN ${viewer}
 WHERE ${kind?`native.kind='${kind}' AND `:''}native.id=${id} AND native.company_id=${company} AND ${hasPermission(actor,'records.read')?'TRUE':'FALSE'} AND ${current} AND (${purchase})
 AND CASE WHEN native.kind='task' THEN ${taskPolicy} ELSE ${member}
 AND (native.kind<>'work_log' OR iam_same_person(p.owner_user_id,${viewer}.user_id,p.company_id) OR iam_same_person(native.source_user_id,${viewer}.user_id,p.company_id))
 AND (native.kind<>'allocation' OR ${hasPermission(actor,'finance.read')?'TRUE':'FALSE'} AND iam_role_allows(${viewer}.role,p.company_id,'finance.read') AND (${viewer}.branch_id IS NULL OR p.branch_id=${viewer}.branch_id) AND (${viewer}.department_id IS NULL OR p.department_id=${viewer}.department_id)) END)`,args:[actor.userId,actor.assignmentId,actor.role,actor.branchId,actor.departmentId]};
}
/** Fixed Evaluation reference aliases retain the ordinary Project/Task source
 * authority. The consumer still proves the employee's actual participation. */
export async function evaluationProjectReadScope(database:SqlDatabase,actor:WorkContext,kind:'project'|'task'){
 const mode=await projectsPackageEnforcementMode();if(mode!=='enforced'){if(mode==='shadow')await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:kind==='project'?'projects.wbs':'projects.tasks'});return {where:'TRUE',args:[] as unknown[]};}
 const alias=kind==='project'?'evaluation_project':'evaluation_task',scope=visibleSource(actor,`${alias}.id`,`${alias}.company_id`,kind);return {where:scope.sql,args:scope.args};
}
/** Actual native identities supplement the generic Module policy. Other Work
 * tasks/references and durable event rows retain their original owners. */
export async function projectSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:'records'|'staff'|'inbox'){
 const mode=await projectsPackageEnforcementMode();if(mode!=='enforced'){
  if(mode==='shadow')await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'projects.wbs'});
  return {where:'TRUE',args:[] as unknown[]};
 }
 const id=surface==='records'?'workspace_records.id':surface==='staff'?'staff_notifications.record_id':'e.source_id',company=surface==='records'?'workspace_records.company_id':surface==='staff'?'staff_notifications.company_id':'e.company_id';
 const native=`EXISTS(SELECT 1 FROM (${nativeSources}) project_source_identity WHERE project_source_identity.id=${id})`,kind=surface==='staff'?'staff_notifications.record_type':'e.source_type';
 const relevant=surface==='records'?native:`(${kind} IN ${sourceKinds} OR ${kind} IN ('task','staff_task','staff_task_alert') AND EXISTS(SELECT 1 FROM staff_tasks owned_task WHERE owned_task.id=${id} AND owned_task.project_id IS NOT NULL))`;
 const visible=visibleSource(actor,id,company);
 const controls=surface==='records'?await projectPurchaseControls(database,actor):undefined;
 const generic=surface==='records'?` AND (module_id<>'projects' OR ${controls?.['projects.wbs'].history?'TRUE':'FALSE'})`:'';
 return {where:`(NOT (${relevant}) OR ${visible.sql})${generic}`,args:visible.args};
}
export async function requireProjectWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 if(await projectsPackageEnforcementMode()==='disabled')return;
 if(moduleId==='projects'){const controls=await projectPurchaseControls(database,actor);if(controls&&!controls['projects.wbs'].history)throw new ApiError(403,'PACKAGE_REQUIRED','Projects history is unavailable.');}
 const exists=await database.prepare(`SELECT 1 FROM (${nativeSources}) native WHERE id=?`).bind(id).first();
 if(!exists)return;
 const mode=await projectsPackageEnforcementMode();if(mode==='shadow'){await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'projects.wbs'});return;}
 const visible=visibleSource(actor,'record.id','record.company_id');
 const allowed=await database.prepare(`SELECT ${visible.sql} allowed FROM (SELECT ?::text id,?::text company_id) record`).bind(...visible.args,id,actor.companyId).first<{allowed:boolean}>();
 if(!allowed?.allowed)throw new ApiError(403,'PROJECT_SOURCE_FORBIDDEN','This Project source is unavailable in the current assignment.');
}
export async function filterProjectWork(database:SqlDatabase,actor:WorkContext,items:WorkItem[]){
 if(await projectsPackageEnforcementMode()==='disabled'||!items.some(item=>item.type==='staff_task'||item.type==='project_handoff'))return items;
 const controls=await projectPurchaseControls(database,actor),handoffs=items.some(item=>item.type==='project_handoff')?new Map((await (await import('@erpx/server-projects/public/project-structure')).projectHandoffWork(actor)).map(item=>[item.id,item])):undefined;
 const output:WorkItem[]=[];
 for(const item of items){
  if(item.type==='project_handoff'){const actual=handoffs?.get(item.id);if(actual)output.push({...item,canAct:item.canAct&&actual.canAct,...(!actual.canAct?{queue:actual.queue,actionKind:'work' as const,flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined}: {})});continue;}
  if(item.type!=='staff_task'){output.push(item);continue;}
  try{const access=await projectTaskPurchaseAccess(database,actor,item.id);if(!access){output.push(item);continue;}
   const owner=await import('@erpx/server-operations/public/staff-tasks'),task=await owner.getTask(database,actor,item.id),state=await owner.taskWorkState(database,actor,task);
   if(state.permittedActions.some(action=>['edit','submit','start','save-progress','complete','approve','reject'].includes(action))&&controls?.['projects.tasks'].history){output.push(item);continue;}
   output.push({...item,canAct:false,actionKind:'work',queue:item.queue==='completed'?'completed':'waiting',flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined,relationships:(item.relationships??[]).filter(value=>!['assigned','approval'].includes(value))});
  }catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))continue;throw error;}
 }
 return output;
}
