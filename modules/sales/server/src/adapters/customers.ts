import { oasisDemoStagingEnabled } from "@erpx/platform-contracts/holding-hosts";
import {followupProjectionScope,followupPurchasedAccess} from "@erpx/sales-server/adapters/followup-purchases";
import {supportWorkspacePurchaseAccess} from '@erpx/support-server/public/support';
import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/commerce-server/public/commercial-owner-projection-sources";
import {salesOrderCounts,salesCustomerOpenOrders,salesSupportOrderSource,salesOrderWorkRows} from "@erpx/commerce-server/public/sales-order-sources";
import {salesCustomerProfile,salesCustomerDirectory} from "@erpx/finance-server/public/sales-customer-sources";
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type {Row} from '@erpx/commerce-server/public/commerce';
import type { WorkItem } from "@erpx/work-projection-ui/lib/work-item";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { legacyModuleCompatibility } from "@erpx/identity-access-server/public/rules/legacy-module-compatibility";
import { salesCustomerAccess, type HubQuery } from "@erpx/sales-server/domain/navigation";
import { availableNavigationAppIds } from "@erpx/work-projection-ui/lib/workplace-catalogue";
import { companyFeatureCatalogue } from "@erpx/pms-server/public/hospitality-rollout";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import {resolveFlow} from '@erpx/workflow-server/public/flow-routing';
import { crmWork } from "@erpx/crm-server/public/crm";
import { customerFollowupWorkspace, customerFollowupWork } from "@erpx/sales-server/adapters/followups";
import { followupCustomerOrderRead } from "./followup-customer-order-read";

import { nativeCustomerNotes } from "@erpx/customer-360-server/public/customer-notes";
import { customerNotesForCustomer } from "@erpx/sales-server/public/customer-notes";
import { headers } from "next/headers";
import { ApiError } from "@erpx/platform-runtime-server/public/http";

import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import {createSalesCustomerApplication,type SalesCustomerRepository} from '@erpx/sales-server/application/customers';
import {salesOrderWorkItems} from '@erpx/sales-server/application/order-work';
import {salesOpportunityCounts,salesOpenOpportunities} from '@erpx/crm-server/public/sales-projection';
import {salesSupportCounts,salesOpenSupportCases} from '@erpx/support-server/public/sales-projection';
import {nativeSalesCustomerReviewCounts} from '@erpx/customer-authority-ui/lib/sales-review-counts';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
export type {CustomerSummary,CustomerDirectoryRow} from '@erpx/sales-server/domain/customers';
function orderScope(actor: WorkContext, alias = "o") {
  return { sql: `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, args: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId] };
}
async function accessContext(actor: WorkContext) {
  const compatibility = legacyModuleCompatibility(actor, actor.modules).flatMap(proof => proof.modules);
  const apps = oasisDemoStagingEnabled(process.env)
    ? ["crm", "sales"]
    : process.env.NODE_ENV === "production" ? availableNavigationAppIds((await companyFeatureCatalogue(actor))
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
async function application(actor:WorkContext){const current = await accessContext(actor);let resource:Promise<SqlDatabase>|null=null;const database=()=>resource??=holdingResources(actor.holdingId).then(x=>x.write),followup=await followupPurchasedAccess(await database(),actor);return createSalesCustomerApplication({authorize:()=>{requirePermission(actor,'commerce.read');},access:async()=>current.access,canReadSupport:hasPermission(actor,'support.read')&&current.purchase.history,canCreateSupport:hasPermission(actor,'support.create')&&current.purchase.newWork,canSell:hasPermission(actor,'commerce.sales')&&(!followup||followup.newWork),clock:{now:()=>Date.now()},repository:async()=>repository(await database(),actor)});}
function repository(write:SqlDatabase,actor:WorkContext):SalesCustomerRepository{const s=orderScope(actor),reader={...actor,pageRequest:undefined};return {
 orderCounts:ids=>salesOrderCounts(write,actor,ids),
 followupHeads:async ids=>{if(!ids.length)return [];const purchase=await followupProjectionScope(write,actor,'c'),placeholders=ids.map(()=>'?').join(',');return (await write.prepare(followupCustomerOrderRead(commerceProjectionSources.commerce_orders,'DISTINCT ON (o.party_id) o.party_id,c.id,c.due_date,c.next_action,c.order_id',s.sql,purchase.where,`o.party_id IN (${placeholders}) AND c.status='open'`,'ORDER BY o.party_id,c.due_date,c.id')).bind(...s.args,...purchase.args, ...ids).all<{ party_id: string; id: string; due_date: string; next_action: string; order_id: string }>()).results;},
 opportunityCounts:ids=>salesOpportunityCounts(write,actor,ids),supportCounts:ids=>salesSupportCounts(write,actor,ids),
 async directory(search,status,cursor){const page=await salesCustomerDirectory(write,actor,search,status,cursor);return {items:page.items.map(row=>({...row,id:row.id as string,page_key:row.page_key as string})),nextCursor:page.nextCursor,previousCursor:page.previousCursor,hasMore:page.hasMore};},
 customer:async id=>salesCustomerProfile(write,actor,id),orders:async id=>salesCustomerOpenOrders(write,actor,id),followups:async id=>{const purchase=await followupProjectionScope(write,actor,'c');return (await write.prepare(followupCustomerOrderRead(commerceProjectionSources.commerce_orders,'c.id,c.next_action,c.due_date,c.status,c.order_id,o.number order_number',s.sql,purchase.where,"o.party_id=? AND c.status='open'",'ORDER BY c.due_date,c.id LIMIT 30')).bind(...s.args,...purchase.args, id).all<Row>()).results;},opportunities:id=>salesOpenOpportunities(write,actor,id),cases:id=>salesOpenSupportCases(write,actor,id),supportSource:id=>salesSupportOrderSource(write,actor,id),
notes:async id=>{let incoming;try{incoming=await headers();}catch{return customerNotesForCustomer(actor,id);}if(!incoming.get('host')||!incoming.get('cookie'))return customerNotesForCustomer(actor,id);return nativeCustomerNotes(incoming,actor,{customerId:id});},reviews:()=>nativeSalesCustomerReviewCounts(actor.holdingSlug,{branchId:actor.branchId??undefined,departmentId:actor.departmentId??undefined}).then(rows=>rows.map(row=>({...row}))),
 crmWork:()=>crmWork(reader),followupWork:()=>customerFollowupWork(reader),orderWork:()=>salesOrderWork(reader),followupIds:async id=>(await customerFollowupWorkspace(reader,id)).items.map(row=>String(row.id)),
};}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function customerSummaries(actor:WorkContext,ids:string[]){return translated(async()=>(await application(actor)).summaries(ids));}
export function customerWorkspaceDirectory(actor:WorkContext,query:HubQuery){return translated(async()=>(await application(actor)).directory(query));}
export function customerWorkspaceProfile(actor:WorkContext,id:string,query:HubQuery={}){return translated(async()=>(await application(actor)).profile(id,query));}
export function customerReviewCounts(actor:WorkContext){return translated(async()=>(await application(actor)).reviews());}
export function salesToday(actor:WorkContext,query:HubQuery){return translated(async()=>(await application(actor)).today(query));}
