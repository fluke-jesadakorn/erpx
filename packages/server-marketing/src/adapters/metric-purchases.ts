import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {commercePackageEnforcementMode,readPurchasedAccesses} from "@erpx/server-commercial/public/purchase-command-boundary";
import {purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {marketingClockedReportFrameSql} from "./marketing-clocked-report-frame";
import {crmMarketingCandidateSources} from "@erpx/server-marketing/adapters/crm-purchase-sources";
import {marketingModuleMemberSql} from "./module-membership";
import {marketingCandidateSourceSql} from "./marketing-candidate-source";
import {actorScopeAssignmentSql} from "./marketing-actor-scope-guard";
export async function marketingMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await commercePackageEnforcementMode(),access=await readPurchasedAccesses(database,mode,actor.companyId,["sales.marketing"]);if(mode!=="enforced")return {"marketing-lead-candidates":true,"marketing-engagement-events":true};
 const current=actor.modules.includes("marketing")&&hasPermission(actor,"records.read")&&!!await database.prepare(actorScopeAssignmentSql(`${assignmentReadSql("marketing_metric_scope","records.read")} AND EXISTS(${marketingModuleMemberSql()})`,'marketing_metric_scope',' allowed')).bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.companyId,actor.branchId,actor.departmentId).first<boolean>("allowed"),allowed=current&&access["sales.marketing"].history;
 return {"marketing-lead-candidates":allowed,"marketing-engagement-events":allowed};
}
export async function marketingMetricSource(database:SqlDatabase,actor:WorkContext,id:string){
 if(id!=="marketing-lead-candidates"&&id!=="marketing-engagement-events")throw new ApiError(422,"METRIC_UNSUPPORTED","This Marketing metric has no original owner source.");
 if(!(await marketingMetricHistoryAccess(database,actor))[id])throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","Original Marketing history and current source authority are required.");
 if(await commercePackageEnforcementMode()!=="enforced")return {sql:marketingCandidateSourceSql,args:[] as unknown[]};
 return {sql:`${marketingClockedReportFrameSql('marketing_metric_clock',' ', 'marketing_metric_owned.*', crmMarketingCandidateSources, 'marketing_metric_owned')}${assignmentReadSql("marketing_metric_owned","records.read")} AND ${purchasedCapabilityPredicate("marketing_metric_owned.company_id","sales.marketing","history","marketing_metric_clock.now_ms")} AND EXISTS(${marketingModuleMemberSql("selected")})`,args:[actor.assignmentId,actor.userId,actor.role,actor.assignmentId]};
}
