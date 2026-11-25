import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {commercePackageEnforcementMode,readPurchasedAccesses} from "@erpx/server-commercial/public/purchase-command-boundary";
import {actorScopeTuple,authorityScopeRead} from "./followup-source-sql";
import {nativeSalesPlan,SALES_RD_FOLLOWUP_METRIC_PROJECTION,SALES_RD_FOLLOWUP_METRIC_SOURCE,SALES_RD_CRM_ASSIGNMENT_ALIASED,SALES_RD_CRM_ASSIGNMENT_PLAIN} from './native-sales-read';
/** One alias-parameterized CRM membership EXISTS shared by the metric history
 * gate (unqualified) and the enforced source projection (`selected` alias).
 * No caller input enters the SQL text; only the alias token changes. */
const crmAssignmentExists=async(alias='')=>(await nativeSalesPlan(alias?SALES_RD_CRM_ASSIGNMENT_ALIASED:SALES_RD_CRM_ASSIGNMENT_PLAIN,[])).sql;
export async function salesFollowupMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await commercePackageEnforcementMode(),access=await readPurchasedAccesses(database,mode,actor.companyId,["sales.quotes","sales.orders"]);if(mode!=="enforced")return {"overdue-followups":true};
 const current=actor.modules.includes("crm")&&hasPermission(actor,"commerce.read")&&!!await database.prepare(authorityScopeRead(assignmentReadSql("sales_metric_scope","commerce.read"),actorScopeTuple,"sales_metric_scope",` AND ${await crmAssignmentExists()}`)).bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.companyId,actor.branchId,actor.departmentId).first<boolean>("allowed");
 return {"overdue-followups":current&&(access["sales.quotes"].history||access["sales.orders"].history)};
}
/** The original quotation/Sales source decides each follow-up capability,
 * before aggregate pagination and evidence limits. */
/** Both source statements are owned by the native command-plan registry; this
 * adapter transports only bounded caller identities and returns the owner-planned
 * statement for the caller's open unit. No TypeScript SQL mirror remains. */
export async function salesFollowupMetricSource(database:SqlDatabase,actor:WorkContext){
 if(!(await salesFollowupMetricHistoryAccess(database,actor))["overdue-followups"])throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","Original Sales history and current source authority are required.");
 const enforced=await commercePackageEnforcementMode()==="enforced";
 const kind=enforced?SALES_RD_FOLLOWUP_METRIC_SOURCE:SALES_RD_FOLLOWUP_METRIC_PROJECTION;
 const values=enforced?[actor.assignmentId,actor.userId,actor.role,actor.assignmentId]:[];
 const planned=await nativeSalesPlan(kind,values);
 return {sql:planned.sql,args:planned.values};
}
