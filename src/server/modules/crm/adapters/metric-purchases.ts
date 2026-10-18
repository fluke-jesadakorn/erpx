import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {ApiError} from "../../runtime/public/http";
import {hasPermission} from "@/lib/identity_access/permissions";
import {commercePackageEnforcementMode,readPurchasedAccesses} from "../../commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "../../commercial/public/purchased-capability-sql";
import {salesFollowupMetricHistoryAccess,salesFollowupMetricSource} from "../../sales/public/metric-purchases";
import {crmPurchaseOpportunityRows} from "./purchases";
const rows=`SELECT lead.* FROM crm_leads lead JOIN (${crmPurchaseOpportunityRows}) actual ON actual.id=lead.id AND actual.company_id=lead.company_id`;
export async function crmMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await commercePackageEnforcementMode(),access=await readPurchasedAccesses(database,mode,actor.companyId,["sales.crm"]),followups=await salesFollowupMetricHistoryAccess(database,actor);if(mode!=="enforced")return {"opportunity-pipeline":true,...followups};
 const current=actor.modules.includes("crm")&&hasPermission(actor,"commerce.read")&&!!await database.prepare(`SELECT ${assignmentReadSql("crm_metric_scope","commerce.read")} AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('crm'::text))) allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) crm_metric_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.companyId,actor.branchId,actor.departmentId).first<boolean>("allowed");
 return {"opportunity-pipeline":current&&access["sales.crm"].history,...followups};
}
export async function crmMetricSource(database:SqlDatabase,actor:WorkContext,id:string){
 if(id==="overdue-followups")return salesFollowupMetricSource(database,actor);
 if(id!=="opportunity-pipeline")throw new ApiError(422,"METRIC_UNSUPPORTED","This CRM metric has no original owner source.");
 if(!(await crmMetricHistoryAccess(database,actor))[id])throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","Original CRM history and current source authority are required.");
 if(await commercePackageEnforcementMode()!=="enforced")return {sql:"SELECT * FROM crm_leads",args:[] as unknown[]};
 return {sql:`WITH crm_metric_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT crm_metric_owned.* FROM (${rows}) crm_metric_owned CROSS JOIN crm_metric_clock WHERE ${assignmentReadSql("crm_metric_owned","commerce.read")} AND ${purchasedCapabilityPredicate("crm_metric_owned.company_id","sales.crm","history","crm_metric_clock.now_ms")} AND EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array('crm'::text)))`,args:[actor.assignmentId,actor.userId,actor.role,actor.assignmentId]};
}
