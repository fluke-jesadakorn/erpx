import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {commercePackageEnforcementMode,readPurchasedAccesses} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {salesFollowupMetricHistoryAccess,salesFollowupMetricSource} from "@erpx/server-sales/public/metric-purchases";
import {actorScopeSql,crmPurchaseOpportunityRows,crmAssignmentModuleExists} from "@erpx/server-crm/adapters/purchases";
import {crmCurrentScopeAllowedSql} from './read-tail-r30-sql';
import {crmAssignmentModuleExistsSql} from './read-tail-r35-sql';
const rows=`SELECT lead.* FROM crm_leads lead JOIN (${crmPurchaseOpportunityRows}) actual ON actual.id=lead.id AND actual.company_id=lead.company_id`;
export async function crmMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await commercePackageEnforcementMode(),access=await readPurchasedAccesses(database,mode,actor.companyId,["sales.crm"]),followups=await salesFollowupMetricHistoryAccess(database,actor);if(mode!=="enforced")return {"opportunity-pipeline":true,...followups};
 const current=actor.modules.includes("crm")&&hasPermission(actor,"commerce.read")&&!!await database.prepare(crmCurrentScopeAllowedSql(assignmentReadSql("crm_metric_scope","commerce.read"),actorScopeSql,"crm_metric_scope",` AND ${crmAssignmentModuleExistsSql('', "'crm'::text)))")}`)).bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.companyId,actor.branchId,actor.departmentId).first<boolean>("allowed");
 return {"opportunity-pipeline":current&&access["sales.crm"].history,...followups};
}
export async function crmMetricSource(database:SqlDatabase,actor:WorkContext,id:string){
 if(id==="overdue-followups")return salesFollowupMetricSource(database,actor);
 if(id!=="opportunity-pipeline")throw new ApiError(422,"METRIC_UNSUPPORTED","This CRM metric has no original owner source.");
 if(!(await crmMetricHistoryAccess(database,actor))[id])throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","Original CRM history and current source authority are required.");
 if(await commercePackageEnforcementMode()!=="enforced")return {sql:"SELECT * FROM crm_leads",args:[] as unknown[]};
 return {sql:`WITH crm_metric_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT crm_metric_owned.* FROM (${rows}) crm_metric_owned CROSS JOIN crm_metric_clock WHERE ${assignmentReadSql("crm_metric_owned","commerce.read")} AND ${purchasedCapabilityPredicate("crm_metric_owned.company_id","sales.crm","history","crm_metric_clock.now_ms")} AND ${crmAssignmentModuleExists}'crm'::text)))`,args:[actor.assignmentId,actor.userId,actor.role,actor.assignmentId]};
}
