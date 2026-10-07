import { commercialOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/commercial-owner-projection-sources";
import {salesOrderCounts,salesCustomerOpenOrders,salesSupportOrderSource,salesOrderWorkRows} from "../../commerce/public/sales-order-sources";
import {salesCustomerProfile,salesCustomerDirectory} from "../../finance/public/sales-customer-sources";
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { Row } from "@/server/modules/commerce/public/commerce";
import type { WorkItem } from "@/lib/work-item";
import { hasPermission } from "@/lib/permissions";
import { salesCustomerAccess, type HubQuery } from "../domain/navigation";
import { availableNavigationAppIds } from "@/lib/workplace-catalogue";
import { companyFeatureCatalogue } from "@/server/modules/pms/public/hospitality-rollout";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { resolveFlow } from "@/server/modules/workflow/public/flow-routing";
import { crmWork } from "@/server/modules/crm/public/crm";
import { customerFollowupWorkspace, customerFollowupWork } from "@/server/customer-followups";
import { customerTimeline } from "@/server/modules/customer_360/public/customer-timeline";
import { customerNotesForCustomer } from "@/server/customer-notes";
import { ApiError } from "@/server/modules/runtime/public/http";


import {BusinessRuleError} from '../../shared';
import {createSalesCustomerApplication,type SalesCustomerRepository} from '../application/customers';
import {salesOrderWorkItems} from '../application/order-work';
import {salesOpportunityCounts,salesOpenOpportunities} from '../../crm/public/sales-projection';
import {salesSupportCounts,salesOpenSupportCases} from '../../support/public/sales-projection';
import {salesCustomerReviewCounts} from '../../customer_authority/public/sales-review-counts';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
export type {CustomerSummary,CustomerDirectoryRow} from '../domain/customers';
function orderScope(actor: WorkContext, alias = "o") {
  return { sql: `${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`, args: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId] };
}
export async function salesCustomersAccess(actor: WorkContext) {
  const apps = process.env.NODE_ENV === "production" ? availableNavigationAppIds(await companyFeatureCatalogue(actor)) : undefined;
  return salesCustomerAccess(actor, apps);
}
async function salesOrderWork(actor: WorkContext): Promise<WorkItem[]> {
  const { write } = await holdingResources(actor.holdingId);
  const rows = await salesOrderWorkRows(write,actor);
  return salesOrderWorkItems(rows,actor.userId,id=>resolveFlow(write,actor,"order",id));
}
function application(actor:WorkContext){let resource:Promise<SqlDatabase>|null=null;const database=()=>resource??=holdingResources(actor.holdingId).then(x=>x.write);return createSalesCustomerApplication({authorize:()=>{requirePermission(actor,'commerce.read');},access:()=>salesCustomersAccess(actor),canReadSupport:hasPermission(actor,'support.read'),canCreateSupport:hasPermission(actor,'support.create'),canSell:hasPermission(actor,'commerce.sales'),timelineUnavailable:error=>error instanceof ApiError&&error.code==='CUSTOMER_TIMELINE_NOT_FOUND',clock:{now:()=>Date.now()},repository:async()=>repository(await database(),actor)});}
function repository(write:SqlDatabase,actor:WorkContext):SalesCustomerRepository{const s=orderScope(actor),reader={...actor,pageRequest:undefined};return {
 orderCounts:ids=>salesOrderCounts(write,actor,ids),
 followupHeads:async ids=>{const placeholders=ids.map(()=>'?').join(',');return (await write.prepare(`SELECT DISTINCT ON (o.party_id) o.party_id,c.id,c.due_date,c.next_action,c.order_id FROM customer_followups c JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE ${s.sql} AND o.party_id IN (${placeholders}) AND c.status='open' ORDER BY o.party_id,c.due_date,c.id`).bind(...s.args, ...ids).all<{ party_id: string; id: string; due_date: string; next_action: string; order_id: string }>()).results;},
 opportunityCounts:ids=>salesOpportunityCounts(write,actor,ids),supportCounts:ids=>salesSupportCounts(write,actor,ids),
 async directory(search,status,cursor){return salesCustomerDirectory(write,actor,search,status,cursor);},
 customer:async id=>salesCustomerProfile(write,actor,id),orders:async id=>salesCustomerOpenOrders(write,actor,id),followups:async id=>(await write.prepare(`SELECT c.id,c.next_action,c.due_date,c.status,c.order_id,o.number order_number FROM customer_followups c JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE ${s.sql} AND o.party_id=? AND c.status='open' ORDER BY c.due_date,c.id LIMIT 30`).bind(...s.args, id).all<Row>()).results,opportunities:id=>salesOpenOpportunities(write,actor,id),cases:id=>salesOpenSupportCases(write,actor,id),supportSource:id=>salesSupportOrderSource(write,actor,id),
 timeline:(id,cursor)=>customerTimeline(reader,id,{cursor}),notes:id=>customerNotesForCustomer(actor,id),reviews:()=>salesCustomerReviewCounts(write,actor),
 crmWork:()=>crmWork(reader),followupWork:()=>customerFollowupWork(reader),orderWork:()=>salesOrderWork(reader),followupIds:async id=>(await customerFollowupWorkspace(reader,id)).items.map(row=>String(row.id)),
};}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function customerSummaries(actor:WorkContext,ids:string[]){return translated(()=>application(actor).summaries(ids));}
export function customerWorkspaceDirectory(actor:WorkContext,query:HubQuery){return translated(()=>application(actor).directory(query));}
export function customerWorkspaceProfile(actor:WorkContext,id:string,query:HubQuery={}){return translated(()=>application(actor).profile(id,query));}
export function customerReviewCounts(actor:WorkContext){return translated(()=>application(actor).reviews());}
export function salesToday(actor:WorkContext,query:HubQuery){return translated(()=>application(actor).today(query));}

