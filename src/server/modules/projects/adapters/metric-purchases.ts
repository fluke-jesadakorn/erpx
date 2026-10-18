import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {ApiError} from "../../runtime/public/http";
import {hasPermission} from "@/lib/identity_access/permissions";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import {projectsPackageEnforcementMode,readPurchasedAccesses} from "../../commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "../../commercial/public/purchased-capability-sql";
import {scheduledFeaturePredicate} from "../../pms/public/hospitality-rollout";
const capabilities={projects:"projects.wbs","overdue-project-milestones":"projects.wbs","project-dependency-blockers":"projects.wbs","project-actual-hours":"projects.time","project-posted-cost":"projects.costs"} as const;
type ProjectMetric=keyof typeof capabilities;
export async function projectMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await projectsPackageEnforcementMode(),access=await readPurchasedAccesses(database,mode,actor.companyId,[...new Set(Object.values(capabilities))]);
 if(mode!=="enforced")return Object.fromEntries(Object.keys(capabilities).map(id=>[id,true])) as Record<ProjectMetric,boolean>;
 const permissions=["records.read","finance.read"],current=new Map<string,boolean>();
 for(const permission of permissions)current.set(permission,hasPermission(actor,permission)&&!!await database.prepare(`SELECT ${assignmentReadSql("project_metric_scope",permission)} allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) project_metric_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first("allowed"));
 return Object.fromEntries(Object.entries(capabilities).map(([id,capability])=>[id,access['projects.wbs'].history&&access[capability].history&&current.get('records.read')&&(id!=="project-posted-cost"||current.get('finance.read'))])) as Record<ProjectMetric,boolean>;
}
export async function requireProjectMetricPurchase(database:SqlDatabase,actor:WorkContext,id:string){if(!(await projectMetricHistoryAccess(database,actor))[id as ProjectMetric])throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","This Project metric requires its purchased source history and current source authority.");}
/** Closed actual Project relation shared by aggregate, pagination, drilldown
 * and evidence reads. Both purchase and selected IAM run before owner LIMIT. */
export async function projectMetricSource(database:SqlDatabase,actor:WorkContext,id:string){
 await requireProjectMetricPurchase(database,actor,id);if(await projectsPackageEnforcementMode()!=="enforced")return {sql:"SELECT * FROM staff_projects",args:[] as unknown[]};
 const capability=capabilities[id as ProjectMetric],cost=id==='project-posted-cost',authority=assignmentReadSql("project_metric_owned","records.read"),finance=cost?assignmentReadSql("project_metric_owned","finance.read"):"TRUE",clock='project_metric_clock.now_ms';
 return {sql:`WITH project_metric_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT project_metric_owned.* FROM staff_projects project_metric_owned CROSS JOIN project_metric_clock WHERE ${authority} AND ${finance} AND ${purchasedCapabilityPredicate('project_metric_owned.company_id','projects.wbs','history',clock)} AND ${purchasedCapabilityPredicate('project_metric_owned.company_id',capability,'history',clock)} AND ${scheduledFeaturePredicate('project_metric_owned.company_id','projects')}`,args:[actor.assignmentId,actor.userId,actor.role,...(cost?[actor.assignmentId,actor.userId,actor.role]:[])]};
}
