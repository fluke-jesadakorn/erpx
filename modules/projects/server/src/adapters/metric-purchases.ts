import "server-only";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {assignmentReadSql} from "@erpx/identity-access-server/public/record-source-authority";
import {projectsPackageEnforcementMode,readPurchasedAccesses} from "@erpx/commercial-server/public/purchase-command-boundary";
import {purchasedCapabilityPredicate} from "@erpx/commercial-server/public/purchased-capability-sql";
import {projectPurchaseSourceHead} from './read-tail-r32';
import { scheduledFeaturePredicate } from "@erpx/pms-server/public/feature-availability";
import {projectRowsSql} from "@erpx/projects-server/adapters/package-obligations";
import {projectAssignmentAuthorityReadSql} from './read-tail-r23';
const capabilities={projects:"projects.wbs","overdue-project-milestones":"projects.wbs","project-dependency-blockers":"projects.wbs","project-actual-hours":"projects.time","project-posted-cost":"projects.costs"} as const;
type ProjectMetric=keyof typeof capabilities;
export async function projectMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await projectsPackageEnforcementMode(),access=await readPurchasedAccesses(database,mode,actor.companyId,[...new Set(Object.values(capabilities))]);
 if(mode!=="enforced")return Object.fromEntries(Object.keys(capabilities).map(id=>[id,true])) as Record<ProjectMetric,boolean>;
 const permissions=["records.read","finance.read"],current=new Map<string,boolean>();
 for(const permission of permissions)current.set(permission,hasPermission(actor,permission)&&!!await database.prepare(projectAssignmentAuthorityReadSql("project_metric_scope",permission)).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first("allowed"));
 return Object.fromEntries(Object.entries(capabilities).map(([id,capability])=>[id,access['projects.wbs'].history&&access[capability].history&&current.get('records.read')&&(id!=="project-posted-cost"||current.get('finance.read'))])) as Record<ProjectMetric,boolean>;
}
export async function requireProjectMetricPurchase(database:SqlDatabase,actor:WorkContext,id:string){if(!(await projectMetricHistoryAccess(database,actor))[id as ProjectMetric])throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","This Project metric requires its purchased source history and current source authority.");}
/** Closed actual Project relation shared by aggregate, pagination, drilldown
 * and evidence reads. Both purchase and selected IAM run before owner LIMIT. */
export async function projectMetricSource(database:SqlDatabase,actor:WorkContext,id:string){
 await requireProjectMetricPurchase(database,actor,id);if(await projectsPackageEnforcementMode()!=="enforced")return {sql:projectRowsSql,args:[] as unknown[]};
 const capability=capabilities[id as ProjectMetric],cost=id==='project-posted-cost',authority=assignmentReadSql("project_metric_owned","records.read"),finance=cost?assignmentReadSql("project_metric_owned","finance.read"):"TRUE",clock='project_metric_clock.now_ms';
 return {sql:`${projectPurchaseSourceHead('project_metric_clock','project_metric_owned','project_metric_owned.*')}${authority} AND ${finance} AND ${purchasedCapabilityPredicate('project_metric_owned.company_id','projects.wbs','history',clock)} AND ${purchasedCapabilityPredicate('project_metric_owned.company_id',capability,'history',clock)} AND ${scheduledFeaturePredicate('project_metric_owned.company_id','projects')}`,args:[actor.assignmentId,actor.userId,actor.role,...(cost?[actor.assignmentId,actor.userId,actor.role]:[])]};
}
