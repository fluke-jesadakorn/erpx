import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {commercePackageEnforcementMode,readPurchasedAccesses} from '@erpx/server-commercial/public/purchase-command-boundary';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import type {FollowupPurchaseControls} from '@erpx/feature-sales/lib/followup-purchases';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import {iamMutationGuards} from '@erpx/server-identity_access/public/iam-store';
import {crmFinanceSourceReadLocks} from '@erpx/server-finance/public/crm-purchase-sources';
import {crmCommerceSourceReadLocks} from '@erpx/server-commerce/public/crm-purchase-sources';
import type {WorkItem} from '@erpx/feature-work_projection/lib/work-item';
import {salesRecipientEligibility} from '@erpx/server-sales/adapters/recipient';
import {actorScopeTuple,authorityScopeRead} from './followup-source-sql';
import {followupCustomerOrderRead} from './followup-customer-order-read';
import {nativeSalesPlan,nativeSalesReadFirst,SALES_RD_FOLLOWUP_METRIC_PREFIX,SALES_RD_FOLLOWUP_SOURCE_FOLLOWUP,SALES_RD_FOLLOWUP_SOURCE_ORDER,SALES_RD_FOLLOWUP_ORDERS,SALES_RD_PURCHASE_PROJECTION_C_READ,SALES_RD_PURCHASE_PROJECTION_C_DENY,SALES_RD_PURCHASE_PROJECTION_FP_READ,SALES_RD_PURCHASE_PROJECTION_FP_DENY} from './native-sales-read';
export type FollowupPurchaseTarget={kind:'followup'|'order';id:string};
type Source={id:string;company_id:string;branch_id:string;department_id:string|null;order_kind:'quote'|'sales';witness:string;owner_id:string|null;due_date:string|null;status:string|null;version:number|null};
const followupOrders=async()=>(await nativeSalesPlan(SALES_RD_FOLLOWUP_ORDERS,[])).sql;
export const followupPurchaseRows=async()=>`${(await nativeSalesPlan(SALES_RD_FOLLOWUP_METRIC_PREFIX,[])).sql},jsonb_build_object('followup',jsonb_build_array(followup.id,followup.company_id,followup.order_id,followup.created_by),'order',actual.witness) witness FROM customer_followups followup JOIN (${await followupOrders()}) actual ON actual.id=followup.order_id AND actual.company_id=followup.company_id`;
/** Owner-local generator for the three follow-up EXISTS guards. Only internally
 * assembled predicates and fixed aliases are accepted, so rendered SQL is
 * byte-identical to the pre-dedupe literals. */
const followupExists=(predicate:string,alias='')=>`SELECT 1 FROM customer_followups${alias?` ${alias}`:''} WHERE ${predicate}`;
const capability=(kind:Source['order_kind'])=>kind==='quote'?'sales.quotes':'sales.orders';
export function followupPurchaseSourceReadLocks(database:SqlDatabase){return [...crmFinanceSourceReadLocks(database),...crmCommerceSourceReadLocks(database),database.prepare('LOCK TABLE customer_followups,customer_followup_events IN SHARE ROW EXCLUSIVE MODE')];}
async function source(database:SqlDatabase,actor:WorkContext,target:FollowupPurchaseTarget){
 /** Owner-local source read: the command-plan registry owns both rendered
  * statements; only bounded target/scope identities cross the wire. */
 const values=[target.id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId];
 const row=target.kind==='followup'
  ? await nativeSalesReadFirst<Source>(SALES_RD_FOLLOWUP_SOURCE_FOLLOWUP,values,database)
  : await nativeSalesReadFirst<Source>(SALES_RD_FOLLOWUP_SOURCE_ORDER,values,database);
 if(!row)throw new ApiError(404,'CUSTOMER_FOLLOWUP_SOURCE_NOT_FOUND','The actual customer quotation or Sales source is unavailable in this workspace.');return row;
}
async function authority(database:SqlDatabase,actor:WorkContext,permissions:readonly string[],record?:Source){
 for(const permission of permissions){if(!hasPermission(actor,permission))throw new ApiError(403,'FOLLOWUP_ACCESS_CHANGED','Choose a current authorized Sales role.');const current=await database.prepare(authorityScopeRead(assignmentReadSql('followup_authority',permission),actorScopeTuple,'followup_authority')).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,record?.branch_id??actor.branchId,record?.department_id??actor.departmentId).first<boolean>('allowed');if(!current)throw new ApiError(403,'FOLLOWUP_ACCESS_CHANGED','The original customer follow-up assignment is no longer current.');}
}
export async function followupPurchasedAccess(database:SqlDatabase,actor:WorkContext,target?:FollowupPurchaseTarget):Promise<FollowupPurchaseControls|undefined>{
 const mode=await commercePackageEnforcementMode();if(mode==='disabled')return undefined;const actual=target?await source(database,actor,target):undefined;await authority(database,actor,['commerce.read'],actual);const values=Object.values(await readPurchasedAccesses(database,mode,actor.companyId,actual?[capability(actual.order_kind)]:['sales.quotes','sales.orders']));return {newWork:values.some(value=>value.newWork),settleExisting:values.some(value=>value.settleExisting),history:values.some(value=>value.history),revision:values.map(value=>value.revision).join(':')};
}
export async function requireFollowupPurchasedRead(database:SqlDatabase,actor:WorkContext,target?:FollowupPurchaseTarget){const flags=await followupPurchasedAccess(database,actor,target);if(flags&&target&&!flags.history)throw new ApiError(403,'PACKAGE_REQUIRED','The actual quotation or Sales package history is required.');return flags;}
/** The caller's original predicates remain; this fixed source clause runs
 * before grouping, pagination or limits. Ordinary CRM module access is not added. */
export async function followupProjectionScope(database:SqlDatabase,actor:WorkContext,alias:'c'|'followup_projection'){
 const mode=await commercePackageEnforcementMode();if(mode!=='enforced'){if(mode==='shadow')await readPurchasedAccesses(database,mode,actor.companyId,['sales.quotes','sales.orders']);return {where:'TRUE',args:[] as unknown[]};}
 const read=hasPermission(actor,'commerce.read');
 const kind=alias==='c'
  ? (read?SALES_RD_PURCHASE_PROJECTION_C_READ:SALES_RD_PURCHASE_PROJECTION_C_DENY)
  : (read?SALES_RD_PURCHASE_PROJECTION_FP_READ:SALES_RD_PURCHASE_PROJECTION_FP_DENY);
 const where=(await nativeSalesPlan(kind,[])).sql;
 return {where,args:[actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,actor.assignmentId,actor.userId,actor.role]};
}
export async function followupSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:'records'|'staff'|'inbox'){
 const mode=await commercePackageEnforcementMode();if(mode!=='enforced'){if(mode==='shadow')await readPurchasedAccesses(database,mode,actor.companyId,['sales.quotes','sales.orders']);return {where:'TRUE',args:[] as unknown[]};}const id=surface==='records'?'workspace_records.id':surface==='staff'?'staff_notifications.record_id':'e.source_id',company=surface==='records'?'workspace_records.company_id':surface==='staff'?'staff_notifications.company_id':'e.company_id',type=surface==='staff'?'staff_notifications.record_type':'e.source_type',actual=`EXISTS(${followupExists(`native.id=${id}`,'native')})`,relevant=surface==='records'?actual:`(${type}='customer_followup' OR ${actual})`,projection=await followupProjectionScope(database,actor,'followup_projection');
 return {where:`(NOT ${relevant} OR EXISTS(${followupExists(`followup_projection.id=${id} AND followup_projection.company_id=${company} AND ${projection.where}`,'followup_projection')}))`,args:projection.args};
}
export async function requireFollowupWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,_moduleId:string,id:string){if(await commercePackageEnforcementMode()==='disabled'||!await database.prepare(followupExists('id=?')).bind(id).first())return;await requireFollowupPurchasedRead(database,actor,{kind:'followup',id});}
export async function filterFollowupPurchasedWork(database:SqlDatabase,actor:WorkContext,items:WorkItem[]){
 const selected=items.filter(item=>item.type==='customer_followup');if(!selected.length||await commercePackageEnforcementMode()!=='enforced')return items;const ids=[...new Set(selected.map(item=>item.id))],projection=await followupProjectionScope(database,actor,'c'),access=await readPurchasedAccesses(database,'enforced',actor.companyId,['sales.quotes','sales.orders']);
 const rows=(await database.prepare(followupCustomerOrderRead(await followupOrders(),`c.id,native.order_kind,${hasPermission(actor,'commerce.sales')?'TRUE':'FALSE'} AND ${assignmentReadSql('native','commerce.sales')} AND c.status='open' AND c.owner_id=? AND ${salesRecipientEligibility()} can_act`,`c.company_id=? AND c.id IN (${ids.map(()=>'?').join(',')})`,projection.where,'','',` JOIN (${await followupPurchaseRows()}) native ON native.id=c.id AND native.company_id=c.company_id`)).bind(actor.assignmentId,actor.userId,actor.role,actor.userId,actor.companyId,...ids,...projection.args).all<{id:string;order_kind:Source['order_kind'];can_act:boolean}>()).results,map=new Map(rows.map(row=>[row.id,row]));
 return items.flatMap(item=>{if(item.type!=='customer_followup')return [item];const row=map.get(item.id);if(!row)return [];if(row.can_act&&access[capability(row.order_kind)].settleExisting)return [item];return [{...item,canAct:false,actionKind:'work' as const,queue:item.queue==='completed'?'completed' as const:'waiting' as const,nextAction:'Review retained customer follow-up',flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined,relationships:(item.relationships??[]).filter(value=>!['assigned','approval'].includes(value))}];});
}
