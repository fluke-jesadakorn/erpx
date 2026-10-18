import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {ApiError} from "../../runtime/public/http";
import {hasPermission} from "@/lib/identity_access/permissions";
import {commercePackageEnforcementMode,readPurchasedAccesses} from "../../commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "../../commercial/public/purchased-capability-sql";
import {crmMarketingCandidateSources} from "./crm-purchase-sources";
export async function marketingMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await commercePackageEnforcementMode(),access=await readPurchasedAccesses(database,mode,actor.companyId,["sales.marketing"]);if(mode!=="enforced")return {"marketing-lead-candidates":true,"marketing-engagement-events":true};
 const current=actor.modules.includes("marketing")&&hasPermission(actor,"records.read")&&!!await database.prepare(`SELECT ${assignmentReadSql("marketing_metric_scope","records.read")} AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text))) allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) marketing_metric_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.companyId,actor.branchId,actor.departmentId).first<boolean>("allowed"),allowed=current&&access["sales.marketing"].history;
 return {"marketing-lead-candidates":allowed,"marketing-engagement-events":allowed};
}
export async function marketingMetricSource(database:SqlDatabase,actor:WorkContext,id:string){
 if(id!=="marketing-lead-candidates"&&id!=="marketing-engagement-events")throw new ApiError(422,"METRIC_UNSUPPORTED","This Marketing metric has no original owner source.");
 if(!(await marketingMetricHistoryAccess(database,actor))[id])throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","Original Marketing history and current source authority are required.");
 if(await commercePackageEnforcementMode()!=="enforced")return {sql:"SELECT * FROM marketing_lead_candidates",args:[] as unknown[]};
 return {sql:`WITH marketing_metric_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT marketing_metric_owned.* FROM (${crmMarketingCandidateSources}) marketing_metric_owned CROSS JOIN marketing_metric_clock WHERE ${assignmentReadSql("marketing_metric_owned","records.read")} AND ${purchasedCapabilityPredicate("marketing_metric_owned.company_id","sales.marketing","history","marketing_metric_clock.now_ms")} AND EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array('marketing'::text)))`,args:[actor.assignmentId,actor.userId,actor.role,actor.assignmentId]};
}
