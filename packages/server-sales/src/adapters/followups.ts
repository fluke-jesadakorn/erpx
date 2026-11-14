import {followupProjectionScope,followupPurchasedAccess,requireFollowupPurchasedRead,filterFollowupPurchasedWork} from '@erpx/server-sales/adapters/followup-purchases';
import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/commercial-owner-projection-sources";
import 'server-only';
import {salesRecipientEligibility} from "@erpx/server-sales/adapters/recipient";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {getOrder} from '@erpx/server-commerce/public/commerce-order-read';
import {type Row} from '@erpx/server-commerce/public/commerce';
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {ApiError} from "@erpx/server-runtime/public/http";

import {BusinessRuleError} from '@erpx/server-shared/index';
import {createFollowupApplication,type FollowupRepository} from '@erpx/server-sales/application/followups';
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
async function list(db:SqlDatabase,a:WorkContext,id?:string,orderId?:string){const purchase=await followupProjectionScope(db,a,'c');return (await db.prepare(`SELECT c.*,(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=c.company_id AND user_id=c.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,o.number order_number,o.branch_id,o.department_id,p.name customer_name,CASE WHEN ${ownerEligible} THEN 1 ELSE 0 END owner_eligible FROM customer_followups c JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=c.order_id AND o.company_id=c.company_id JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id AND p.company_id=o.company_id WHERE ${scope} AND ${purchase.where} AND (?::text IS NULL OR c.id=?) AND (?::text IS NULL OR c.order_id=?) ORDER BY c.due_date,c.id`).bind(...args(a),...purchase.args,id??null,id??null,orderId??null,orderId??null).all<Row>()).results;}

async function recipients(db:SqlDatabase,a:WorkContext,orderId:string){return (await db.prepare(`SELECT DISTINCT c.owner_id id,p.legal_name name FROM (SELECT DISTINCT user_id owner_id FROM (${identity_accessProjectionSources.assignments}) assignments WHERE company_id=?) c JOIN (${people_payrollProjectionSources.hr_people}) p ON p.user_id=c.owner_id AND p.company_id=? JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=? WHERE ${scope} AND ${ownerEligible} ORDER BY p.legal_name`).bind(a.companyId,a.companyId,orderId,...args(a)).all<{id:string;name:string}>()).results;}
function application(a:WorkContext,provided?:SqlDatabase){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);return createFollowupApplication({actorId:a.userId,canRead:hasPermission(a,'commerce.read'),canSell:hasPermission(a,'commerce.sales'),canManage:hasPermission(a,'commerce.approve'),authorize:permission=>{requirePermission(a,permission);},clock:{now:()=>Date.now()},repository:async()=>repository(await database(),a)});}
function repository(write:SqlDatabase,a:WorkContext):FollowupRepository{return {
 rows:(id,orderId)=>list(write,a,id,orderId),
 async evidence(id,from,to,beforeDate){const purchase=await followupProjectionScope(write,a,'c');return await write.prepare(`SELECT c.id,c.order_id,c.next_action,c.due_date,c.status,c.version,o.number order_number FROM customer_followups c
 JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=c.order_id AND o.company_id=c.company_id JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id AND p.company_id=o.company_id
 WHERE ${scope} AND ${purchase.where} AND c.id=? AND o.kind IN ('sales','quote') AND c.status='open' AND c.due_date BETWEEN ? AND ? AND c.due_date<? LIMIT 1`)
 .bind(...args(a),...purchase.args,id,from,to,beforeDate).first<Row>();},
 recipients:orderId=>recipients(write,a,orderId),history:async id=>(await write.prepare('SELECT * FROM customer_followup_events WHERE followup_id=? ORDER BY version DESC').bind(id).all<Row>()).results,order:async id=>(await getOrder(a,id,write)).order,
 };}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function getCustomerFollowup(a:WorkContext,id:string){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write;await requireFollowupPurchasedRead(db,a,{kind:'followup',id});return application(a,db).get(id);});}
export function customerFollowupEvidenceRow(a:WorkContext,id:string,from:string,to:string,beforeDate:string,provided?:SqlDatabase){return translated(async()=>{const db=provided??(await holdingResources(a.holdingId)).write;await requireFollowupPurchasedRead(db,a,{kind:'followup',id});return application(a,db).evidence(id,from,to,beforeDate);});}
export function customerFollowupWorkspace(a:WorkContext,orderId?:string){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write,purchase=await requireFollowupPurchasedRead(db,a,orderId?{kind:'order',id:orderId}:undefined),view=await application(a,db).workspace(orderId);return {...view,...(purchase?{purchase,canCreate:view.canCreate&&purchase.newWork}: {})};});}
export function customerFollowupDetail(a:WorkContext,id:string){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write,purchase=await requireFollowupPurchasedRead(db,a,{kind:'followup',id}),view=await application(a,db).detail(id);return {...view,...(purchase?{purchase}: {})};});}
export function customerFollowupWork(a:WorkContext){return translated(async()=>{if(!hasPermission(a,'commerce.read'))return [];const db=(await holdingResources(a.holdingId)).write,purchase=await followupPurchasedAccess(db,a);if(purchase&&!purchase.history)return [];return filterFollowupPurchasedWork(db,a,await application(a,db).work());});}
