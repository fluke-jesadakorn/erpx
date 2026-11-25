import "server-only";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";

import {projectsPackageEnforcementMode,readPurchasedAccess} from "@erpx/server-commercial/public/purchase-command-boundary";
import {purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {projectTaskRecordPolicy} from "@erpx/server-operations/public/project-task-record-policy";
import {projectOwnerOrMemberColumnSql} from "@erpx/server-projects/adapters/project-read-projections";
import {scopeBindProjection} from './read-tail-r39';
import {selectBodySql} from './read-tail-r45';

/** Byte-identical source identity projection shared by the native source arms;
 * one definition keeps every UNION column list identical. */
const nativeSourceColumns=selectBodySql('id,project_id,company_id,');
export const nativeSources=`SELECT id,id project_id,company_id,'project' kind,'projects.wbs' capability,NULL::text source_user_id FROM staff_projects
 UNION ALL ${nativeSourceColumns}'milestone','projects.wbs',NULL::text FROM staff_project_milestones
 UNION ALL ${nativeSourceColumns}'work_log','projects.time',user_id FROM staff_project_work_logs
 UNION ALL ${nativeSourceColumns}'allocation','projects.costs',NULL::text FROM staff_project_cost_allocations
 UNION ALL ${nativeSourceColumns}'handoff','projects.handover',NULL::text FROM staff_project_handoffs
 UNION ALL ${nativeSourceColumns}'task','projects.tasks',NULL::text FROM staff_tasks WHERE project_id IS NOT NULL`;
export function visibleSource(actor:WorkContext,id:string,company:string,kind?:'project'|'task'){
 const viewer='project_source_viewer',taskPolicy=projectTaskRecordPolicy('native.id','native.company_id',viewer);
 const current=`EXISTS(SELECT 1 FROM assignments g WHERE g.id=${viewer}.assignment_id AND g.user_id=${viewer}.user_id AND g.role=${viewer}.role AND g.company_id=native.company_id
 AND iam_assignment_active(g.id,${viewer}.now_ms) AND iam_principal_active(g.user_id,g.company_id,${viewer}.now_ms) AND iam_role_allows(g.role,g.company_id,'records.read')
 AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${viewer}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${viewer}.department_id))`;
 const member=projectOwnerOrMemberColumnSql('p',`${viewer}.user_id`,'m');
 const purchase=['projects.wbs','projects.tasks','projects.time','projects.costs','projects.handover'].map(code=>`(native.capability='${code}' AND ${purchasedCapabilityPredicate('native.company_id',code,'history',`${viewer}.now_ms`)})`).join(' OR ');
 return {sql:`EXISTS(WITH ${viewer} AS MATERIALIZED ${scopeBindProjection('viewer-clock')}
 SELECT 1 FROM (${nativeSources}) native JOIN staff_projects p ON p.id=native.project_id AND p.company_id=native.company_id CROSS JOIN ${viewer}
 WHERE ${kind?`native.kind='${kind}' AND `:''}native.id=${id} AND native.company_id=${company} AND ${hasPermission(actor,'records.read')?'TRUE':'FALSE'} AND ${current} AND (${purchase})
 AND CASE WHEN native.kind='task' THEN ${taskPolicy} ELSE ${member}
 AND (native.kind<>'work_log' OR iam_same_person(p.owner_user_id,${viewer}.user_id,p.company_id) OR iam_same_person(native.source_user_id,${viewer}.user_id,p.company_id))
 AND (native.kind<>'allocation' OR ${hasPermission(actor,'finance.read')?'TRUE':'FALSE'} AND iam_role_allows(${viewer}.role,p.company_id,'finance.read') AND (${viewer}.branch_id IS NULL OR p.branch_id=${viewer}.branch_id) AND (${viewer}.department_id IS NULL OR p.department_id=${viewer}.department_id)) END)`,args:[actor.userId,actor.assignmentId,actor.role,actor.branchId,actor.departmentId]};
}
export async function evaluationProjectReadScope(database:SqlDatabase,actor:WorkContext,kind:'project'|'task'){
 const mode=await projectsPackageEnforcementMode();if(mode!=='enforced'){if(mode==='shadow')await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:kind==='project'?'projects.wbs':'projects.tasks'});return {where:'TRUE',args:[] as unknown[]};}
 const alias=kind==='project'?'evaluation_project':'evaluation_task',scope=visibleSource(actor,`${alias}.id`,`${alias}.company_id`,kind);return {where:scope.sql,args:scope.args};
}
