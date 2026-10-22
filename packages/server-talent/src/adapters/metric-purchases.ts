import "server-only";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {talentPackageEnforcementMode,readPurchasedAccesses} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
const metrics={"open-requisitions":"recruit.positions","application-stage-counts":"recruit.applications"} as const;
export async function talentMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await talentPackageEnforcementMode(),access=await readPurchasedAccesses(database,mode,actor.companyId,["recruit.positions","recruit.applications"]);if(mode!=="enforced")return {"open-requisitions":true,"application-stage-counts":true};
 const current=hasModuleAccess(actor.modules,"talent")&&hasPermission(actor,"records.read")&&hasPermission(actor,"records.review")&&!!await database.prepare(`SELECT ${assignmentReadSql("talent_metric_scope","records.read")} AND ${assignmentReadSql("talent_metric_scope","records.review")} allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) talent_metric_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first<boolean>("allowed");
 return {"open-requisitions":current&&access["recruit.positions"].history,"application-stage-counts":current&&access["recruit.applications"].history};
}
/** Aggregate and source-page SQL share the original rows, actual Company
 * lineage and both current Talent privacy permissions. */
export async function talentMetricSource(database:SqlDatabase,actor:WorkContext,id:string){
 const capability=metrics[id as keyof typeof metrics];if(!capability)throw new ApiError(422,"METRIC_UNSUPPORTED","This Recruitment metric has no owner source.");
 if(!(await talentMetricHistoryAccess(database,actor))[id as keyof typeof metrics])throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","Recruitment history and current reviewer authority are required.");
 const rows=id==="open-requisitions"?"SELECT * FROM talent_requisitions":"SELECT a.*,r.branch_id,r.department_id FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id";
 if(await talentPackageEnforcementMode()!=="enforced")return {sql:rows,args:[] as (string|number|null)[]};
 return {sql:`WITH talent_metric_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT talent_metric_owned.* FROM (${rows}) talent_metric_owned CROSS JOIN talent_metric_clock WHERE ${assignmentReadSql("talent_metric_owned","records.read")} AND ${assignmentReadSql("talent_metric_owned","records.review")} AND ${purchasedCapabilityPredicate("talent_metric_owned.company_id",capability,"history","talent_metric_clock.now_ms")}`,args:[actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role]};
}
