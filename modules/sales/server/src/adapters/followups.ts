import {followupProjectionScope,followupPurchasedAccess,requireFollowupPurchasedRead,filterFollowupPurchasedWork} from '@erpx/sales-server/adapters/followup-purchases';
import {pointRead} from './point-read';
import {nativeSalesPlan,SALES_RD_OWNER_NAME_PROJECTION,SALES_RD_FOLLOWUP_EVIDENCE_PREFIX} from './native-sales-read';
import {nativeSalesReadAll,SALES_RD_FOLLOWUP_RECIPIENTS} from './native-sales-read';
import {followupCustomerOrderTailRead} from './followup-customer-order-tail-read';
import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/commerce-server/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/finance-server/public/commercial-owner-projection-sources";
import 'server-only';
import {salesRecipientEligibility} from "@erpx/sales-server/adapters/recipient";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {getOrder} from '@erpx/commerce-server/public/commerce-order-read';
import {type Row} from '@erpx/commerce-server/public/commerce';
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {requirePermission} from "@erpx/identity-access-server/public/iam-permissions";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {ApiError} from "@erpx/platform-runtime-server/public/http";

import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import {createFollowupApplication,type FollowupRepository} from '@erpx/sales-server/application/followups';
/**
 * Retained customer follow-up read projections. Create/reassign/complete/cancel
 * are native-owned (`customer_followups_router`); this module keeps only the
 * workspace/detail/evidence/Work reads consumed by presenters, Work projection,
 * notifications and executive evidence.
 */
const scope="o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)";
const args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
// Current account authority and effective employment both determine whether the recipient can take work.
const ownerEligible=salesRecipientEligibility();
async function list(db:SqlDatabase,a:WorkContext,id?:string,orderId?:string){const purchase=await followupProjectionScope(db,a,'c'),ownerName=(await nativeSalesPlan(SALES_RD_OWNER_NAME_PROJECTION,[])).sql;return (await db.prepare(followupCustomerOrderTailRead(commerceProjectionSources.commerce_orders,`c.*,${ownerName},o.number order_number,o.branch_id,o.department_id,p.name customer_name,CASE WHEN ${ownerEligible} THEN 1 ELSE 0 END owner_eligible`,scope,purchase.where,`(?::text IS NULL OR c.id=?) AND (?::text IS NULL OR c.order_id=?)`,'ORDER BY c.due_date,c.id',` JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id AND p.company_id=o.company_id`)).bind(...args(a),...purchase.args,id??null,id??null,orderId??null,orderId??null).all<Row>()).results;}

/** Owner-local recipient read: the axum `command-plan` registry owns the statement;
 * the caller sends only the bounded Company/order identities and runs the planned
 * statement on its already-open unit. No TypeScript SQL mirror remains. */
async function recipients(db:SqlDatabase,a:WorkContext,orderId:string){return (await nativeSalesReadAll<{id:string;name:string}>(SALES_RD_FOLLOWUP_RECIPIENTS,[a.companyId,a.companyId,orderId,...args(a)],db)).results;}
function application(a:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);return createFollowupApplication({actorId:a.userId,canRead:hasPermission(a,'commerce.read'),canSell:hasPermission(a,'commerce.sales'),canManage:hasPermission(a,'commerce.approve'),authorize:permission=>{requirePermission(a,permission);},clock:{now:()=>Date.now()},repository:async()=>repository(await database(),a)});}
function repository(write:SqlDatabase,a:WorkContext):FollowupRepository{return {
 rows:(id,orderId)=>list(write,a,id,orderId),
 async evidence(id,from,to,beforeDate){const purchase=await followupProjectionScope(write,a,'c'),ownerName=(await nativeSalesPlan(SALES_RD_OWNER_NAME_PROJECTION,[])).sql,evidencePrefix=(await nativeSalesPlan(SALES_RD_FOLLOWUP_EVIDENCE_PREFIX,[])).sql;return await write.prepare(`${evidencePrefix}customer_followups c
 JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=c.order_id AND o.company_id=c.company_id JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id AND p.company_id=o.company_id
 WHERE ${scope} AND ${purchase.where} AND c.id=? AND o.kind IN ('sales','quote') AND c.status='open' AND c.due_date BETWEEN ? AND ? AND c.due_date<? LIMIT 1`)
 .bind(...args(a),...purchase.args,id,from,to,beforeDate).first<Row>();},
 recipients:orderId=>recipients(write,a,orderId),history:async id=>(await write.prepare(pointRead('*','customer_followup_events','followup_id',' ORDER BY version DESC')).bind(id).all<Row>()).results,order:async id=>(await getOrder(a,id,write)).order,
 };}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function getCustomerFollowup(a:WorkContext,id:string){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write;await requireFollowupPurchasedRead(db,a,{kind:'followup',id});return application(a,db).get(id);});}
export function customerFollowupEvidenceRow(a:WorkContext,id:string,from:string,to:string,beforeDate:string,provided?:SqlDatabase){return translated(async()=>{const db=provided??(await holdingResources(a.holdingId)).write;await requireFollowupPurchasedRead(db,a,{kind:'followup',id});return application(a,db).evidence(id,from,to,beforeDate);});}
export function customerFollowupWorkspace(a:WorkContext,orderId?:string){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write,purchase=await requireFollowupPurchasedRead(db,a,orderId?{kind:'order',id:orderId}:undefined),view=await application(a,db).workspace(orderId);return {...view,...(purchase?{purchase,canCreate:view.canCreate&&purchase.newWork}: {})};});}
export function customerFollowupDetail(a:WorkContext,id:string){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write,purchase=await requireFollowupPurchasedRead(db,a,{kind:'followup',id}),view=await application(a,db).detail(id);return {...view,...(purchase?{purchase}: {})};});}
export function customerFollowupWork(a:WorkContext){return translated(async()=>{if(!hasPermission(a,'commerce.read'))return [];const db=(await holdingResources(a.holdingId)).write,purchase=await followupPurchasedAccess(db,a);if(purchase&&!purchase.history)return [];return filterFollowupPurchasedWork(db,a,await application(a,db).work());});}
