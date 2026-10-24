import {followupProjectionScope,followupPurchasedAccess} from "@erpx/server-sales/adapters/followup-purchases";
import { supportWorkspacePurchaseAccess } from "@erpx/server-support/public/support";
import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";
import {salesOrderCounts,salesCustomerOpenOrders,salesSupportOrderSource,salesOrderWorkRows} from "@erpx/server-commerce/public/sales-order-sources";
import {salesCustomerProfile,salesCustomerDirectory} from "@erpx/server-finance/public/sales-customer-sources";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { Row } from "@erpx/server-commerce/public/commerce";
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { legacyModuleCompatibility } from "@erpx/server-identity_access/public/rules/legacy-module-compatibility";
import { salesCustomerAccess, type HubQuery } from "@erpx/server-sales/domain/navigation";
import { availableNavigationAppIds } from "@erpx/feature-work_projection/lib/workplace-catalogue";
import { companyFeatureCatalogue } from "@erpx/server-pms/public/hospitality-rollout";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import {resolveFlow} from '@erpx/server-workflow/public/flow-routing';
import { crmWork } from "@erpx/server-crm/public/crm";
import { customerFollowupWorkspace, customerFollowupWork } from "@erpx/server-sales/adapters/followups";
import { customerTimeline } from "@erpx/server-customer_360/public/customer-timeline";
import { customerNotesForCustomer } from "@erpx/server-sales/public/customer-notes";
import { ApiError } from "@erpx/server-runtime/public/http";


import {BusinessRuleError} from '@erpx/server-shared/index';
import {createSalesCustomerApplication,type SalesCustomerRepository} from '@erpx/server-sales/application/customers';
import {salesOrderWorkItems} from '@erpx/server-sales/application/order-work';
import {salesOpportunityCounts,salesOpenOpportunities} from '@erpx/server-crm/public/sales-projection';
import {salesSupportCounts,salesOpenSupportCases} from '@erpx/server-support/public/sales-projection';
import {salesCustomerReviewCounts} from '@erpx/server-customer_authority/public/sales-review-counts';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export type {CustomerSummary,CustomerDirectoryRow} from '@erpx/server-sales/domain/customers';
function orderScope(actor: WorkContext, alias = "o") {
  return { sql: `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, args: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId] };
}
async function accessContext(actor: WorkContext) {
  const compatibility = legacyModuleCompatibility(actor, actor.modules).flatMap(proof => proof.modules);
  const apps = process.env.NODE_ENV === "production" ? availableNavigationAppIds((await companyFeatureCatalogue(actor))
    .map(entry => compatibility.includes(entry.id) ? { ...entry, permitted: true } : entry)) : undefined;
  const access = salesCustomerAccess(actor, apps), purchase = await supportWorkspacePurchaseAccess(actor);
  return { access: { ...access, support: access.support && purchase.history }, purchase };
}
export async function salesCustomersAccess(actor: WorkContext) { return (await accessContext(actor)).access; }
async function salesOrderWork(actor: WorkContext): Promise<WorkItem[]> {
  const { write } = await holdingResources(actor.holdingId);
  const rows = await salesOrderWorkRows(write,actor);
  return salesOrderWorkItems(rows,actor.userId,id=>resolveFlow(write,actor,"order",id));
}
async function application(actor:WorkContext){const current = await accessContext(actor);let resource:Promise<SqlDatabase>|null=null;const database=()=>resource??=holdingResources(actor.holdingId).then(x=>x.write),followup=await followupPurchasedAccess(await database(),actor);return createSalesCustomerApplication({authorize:()=>{requirePermission(actor,'commerce.read');},access:async()=>current.access,canReadSupport:hasPermission(actor,'support.read')&&current.purchase.history,canCreateSupport:hasPermission(actor,'support.create')&&current.purchase.newWork,canSell:hasPermission(actor,'commerce.sales')&&(!followup||followup.newWork),timelineUnavailable:error=>error instanceof ApiError&&error.code==='CUSTOMER_TIMELINE_NOT_FOUND',clock:{now:()=>Date.now()},repository:async()=>repository(await database(),actor)});}
function repository(write:SqlDatabase,actor:WorkContext):SalesCustomerRepository{const s=orderScope(actor),reader={...actor,pageRequest:undefined};return {
 orderCounts:ids=>salesOrderCounts(write,actor,ids),
 followupHeads:async ids=>{if(!ids.length)return [];const purchase=await followupProjectionScope(write,actor,'c'),placeholders=ids.map(()=>'?').join(',');return (await write.prepare(`SELECT DISTINCT ON (o.party_id) o.party_id,c.id,c.due_date,c.next_action,c.order_id FROM customer_followups c JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE ${s.sql} AND ${purchase.where} AND o.party_id IN (${placeholders}) AND c.status='open' ORDER BY o.party_id,c.due_date,c.id`).bind(...s.args,...purchase.args, ...ids).all<{ party_id: string; id: string; due_date: string; next_action: string; order_id: string }>()).results;},
 opportunityCounts:ids=>salesOpportunityCounts(write,actor,ids),supportCounts:ids=>salesSupportCounts(write,actor,ids),
 async directory(search,status,cursor){return salesCustomerDirectory(write,actor,search,status,cursor);},
 customer:async id=>salesCustomerProfile(write,actor,id),orders:async id=>salesCustomerOpenOrders(write,actor,id),followups:async id=>{const purchase=await followupProjectionScope(write,actor,'c');return (await write.prepare(`SELECT c.id,c.next_action,c.due_date,c.status,c.order_id,o.number order_number FROM customer_followups c JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE ${s.sql} AND ${purchase.where} AND o.party_id=? AND c.status='open' ORDER BY c.due_date,c.id LIMIT 30`).bind(...s.args,...purchase.args, id).all<Row>()).results;},opportunities:id=>salesOpenOpportunities(write,actor,id),cases:id=>salesOpenSupportCases(write,actor,id),supportSource:id=>salesSupportOrderSource(write,actor,id),
 timeline:(id,cursor)=>customerTimeline(reader,id,{cursor}),notes:id=>customerNotesForCustomer(actor,id),reviews:()=>salesCustomerReviewCounts(write,actor),
 crmWork:()=>crmWork(reader),followupWork:()=>customerFollowupWork(reader),orderWork:()=>salesOrderWork(reader),followupIds:async id=>(await customerFollowupWorkspace(reader,id)).items.map(row=>String(row.id)),
};}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function customerSummaries(actor:WorkContext,ids:string[]){return translated(async()=>(await application(actor)).summaries(ids));}
export function customerWorkspaceDirectory(actor:WorkContext,query:HubQuery){return translated(async()=>(await application(actor)).directory(query));}
export function customerWorkspaceProfile(actor:WorkContext,id:string,query:HubQuery={}){return translated(async()=>(await application(actor)).profile(id,query));}
export function customerReviewCounts(actor:WorkContext){return translated(async()=>(await application(actor)).reviews());}
export function salesToday(actor:WorkContext,query:HubQuery){return translated(async()=>(await application(actor)).today(query));}

