import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {ApiError} from "../../runtime/public/http";
import {hasPermission} from "@/lib/identity_access/permissions";
import {commercePackageEnforcementMode,readPurchasedAccesses} from "../../commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "../../commercial/public/purchased-capability-sql";
import {crmCommerceOrderSources} from "../../commerce/public/crm-purchase-sources";
import {crmFinanceCustomerSources} from "../../finance/public/crm-purchase-sources";
const rows=`SELECT followup.*,actual.branch_id,actual.department_id,actual.kind order_kind FROM customer_followups followup JOIN (${crmCommerceOrderSources}) actual ON actual.id=followup.order_id AND actual.company_id=followup.company_id JOIN (${crmFinanceCustomerSources}) customer ON customer.id=actual.party_id AND customer.company_id=actual.company_id WHERE actual.kind IN ('sales','quote')`;
export async function salesFollowupMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await commercePackageEnforcementMode(),access=await readPurchasedAccesses(database,mode,actor.companyId,["sales.quotes","sales.orders"]);if(mode!=="enforced")return {"overdue-followups":true};
 const current=actor.modules.includes("crm")&&hasPermission(actor,"commerce.read")&&!!await database.prepare(`SELECT ${assignmentReadSql("sales_metric_scope","commerce.read")} AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('crm'::text))) allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) sales_metric_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.companyId,actor.branchId,actor.departmentId).first<boolean>("allowed");
 return {"overdue-followups":current&&(access["sales.quotes"].history||access["sales.orders"].history)};
}
/** The original quotation/Sales source decides each follow-up capability,
 * before aggregate pagination and evidence limits. */
export async function salesFollowupMetricSource(database:SqlDatabase,actor:WorkContext){
 if(!(await salesFollowupMetricHistoryAccess(database,actor))["overdue-followups"])throw new ApiError(403,"METRIC_SOURCE_UNAVAILABLE","Original Sales history and current source authority are required.");
 if(await commercePackageEnforcementMode()!=="enforced")return {sql:`SELECT followup.*,actual.branch_id,actual.department_id,actual.kind order_kind FROM customer_followups followup JOIN (${crmCommerceOrderSources}) actual ON actual.id=followup.order_id AND actual.company_id=followup.company_id`,args:[] as unknown[]};
 return {sql:`WITH sales_metric_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT sales_metric_owned.* FROM (${rows}) sales_metric_owned CROSS JOIN sales_metric_clock WHERE ${assignmentReadSql("sales_metric_owned","commerce.read")} AND ((sales_metric_owned.order_kind='quote' AND ${purchasedCapabilityPredicate("sales_metric_owned.company_id","sales.quotes","history","sales_metric_clock.now_ms")}) OR (sales_metric_owned.order_kind='sales' AND ${purchasedCapabilityPredicate("sales_metric_owned.company_id","sales.orders","history","sales_metric_clock.now_ms")})) AND EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array('crm'::text)))`,args:[actor.assignmentId,actor.userId,actor.role,actor.assignmentId]};
}
