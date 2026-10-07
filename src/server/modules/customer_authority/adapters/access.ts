import { finalRootOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as financeProjectionSources } from "../../finance/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "../../identity_access/public/final-root-owner-projection-sources";
import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {Row} from "../../commerce/public/commerce";
import {holdingResources} from "../../runtime/public/runtime";
import {requirePermission} from "../../identity_access/public/iam-permissions";
import {hasPermission} from "@/lib/permissions";
import {assertion,executeCommand,replayCommand} from "../../workflow/public/commands";
import {ApiError} from "../../runtime/public/http";
import {prepareCustomerAccessNotice1,prepareCustomerAccessNotice2} from "../../collaboration/public/customer-authority-notifications";
import {getOrder} from "../../commerce/public/commerce";
import {salesRecipientEligibility} from "../../sales/public/sales-recipient";
import {createCustomerAccessApplication,type AccessRepository} from "../application/access";
const scope="o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)";
const args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
const eligible=salesRecipientEligibility().replaceAll('c.owner_id','m.user_id');
const reviewers=`m.company_id=o.company_id AND m.user_id<>b.created_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'commerce.approve') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=o.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=o.department_id)) AND ${eligible}`;
const approvedSource="o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')";
function ports(a:WorkContext) { return {actorId:a.userId,companyScope:!a.branchId&&!a.departmentId,require:(permission:"commerce.read"|"commerce.sales"|"commerce.approve")=>requirePermission(a,permission),canApprove:()=>hasPermission(a,'commerce.approve'),canSales:()=>hasPermission(a,'commerce.sales'),canRead:()=>hasPermission(a,'commerce.read'),clock:{now:()=>Date.now()},error:(status:number,code:string,message:string)=>new ApiError(status,code,message)}; }
function repository(a:WorkContext,write:SqlDatabase):AccessRepository { return {
 binding:id=>write.prepare(`SELECT b.*,o.number,o.branch_id,o.department_id,CASE WHEN EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) m WHERE ${reviewers} AND m.user_id=?) THEN 1 ELSE 0 END actor_eligible FROM customer_access_bindings b JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id WHERE b.id=? AND ${scope}`).bind(a.userId,id,...args(a)).first<Row>(),
 rows:async()=> (await write.prepare(`SELECT b.*,o.number,CASE WHEN EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) m WHERE ${reviewers} AND m.user_id=?) THEN 1 ELSE 0 END actor_eligible,(SELECT COUNT(DISTINCT m.user_id) FROM (${identity_accessProjectionSources.assignments}) m WHERE ${reviewers}) reviewers FROM customer_access_bindings b JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id WHERE ${scope} ORDER BY b.updated_at DESC,b.id`).bind(a.userId,...args(a)).all<Row>()).results,
 sources:async()=> (await write.prepare(`SELECT o.id,o.number,o.version,p.name FROM (${commerceProjectionSources.commerce_orders}) o JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id AND p.company_id=o.company_id WHERE ${scope} AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled') ORDER BY o.updated_at DESC LIMIT 250`).bind(...args(a)).all<Row>()).results,
 replay:({key,operation,id,input})=>replayCommand(write,a,key,operation,id,input),
 order:async id=>(await getOrder(a,id,write)).order,
 request:async(command,order,now)=>{const {key,operation:op,id,input}=command;return executeCommand(write,a,key,op,input,id,1,{id,version:1,status:'pending'},[
 assertion(write,key,'source',`EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_orders}) o WHERE o.id=? AND o.version=? AND ${scope} AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled'))`,[input.orderId,input.sourceVersion,...args(a)]),
 write.prepare('INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,reason,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(id,a.companyId,input.orderId,order.party_id,input.email,input.expiresAt,input.reason,a.userId,now,now),
 write.prepare("INSERT INTO customer_access_events VALUES(?,?,1,'request',?,?,?)").bind(`customer-access-event-${key}`,id,a.userId,input.reason,now),
 prepareCustomerAccessNotice1(write,key,id)
 ],{companyId:a.companyId,branchId:String(order.branch_id),departmentId:order.department_id?String(order.department_id):null});},
 change:async(command,row,facts)=>{const {key,operation:op,id,input}=command,{review,status,now}=facts;return executeCommand(write,a,key,op,input,id,input.expectedVersion+1,{id,version:input.expectedVersion+1,status},[
 assertion(write,key,'binding',`EXISTS(SELECT 1 FROM customer_access_bindings b JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=b.order_id WHERE b.id=? AND b.version=? AND ${scope} AND b.status IN (${review ? "'pending'" : "'pending','approved'"}) ${review ? 'AND b.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint' : ''})`,[id,input.expectedVersion,...args(a)]),
 ...(review?[assertion(write,key,'reviewer',`EXISTS(SELECT 1 FROM customer_access_bindings b JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=b.order_id JOIN (${identity_accessProjectionSources.assignments}) m ON ${reviewers} WHERE b.id=? AND m.user_id=?)`,[id,a.userId])]:[]),
 ...(input.action==='approve'?[assertion(write,key,'publishable-source',`EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_orders}) o WHERE o.id=? AND ${approvedSource})`,[String(row.order_id)])]:[]),
 write.prepare('UPDATE customer_access_bindings SET status=?,approved_by=?,reviewed_at=?,reason=?,version=version+1,updated_at=? WHERE id=?').bind(status,input.action==='approve'?a.userId:row.approved_by,review?now:row.reviewed_at,input.reason,now,id),
 write.prepare('INSERT INTO customer_access_events VALUES(?,?,?,?,?,?,?)').bind(`customer-access-event-${key}`,id,input.expectedVersion+1,input.action,a.userId,input.reason,now),
 prepareCustomerAccessNotice2(write,key,id)
 ],{companyId:a.companyId,branchId:String(row.branch_id),departmentId:row.department_id?String(row.department_id):null});},
 events:async id=>(await write.prepare('SELECT version,action,actor_id,reason,created_at FROM customer_access_events WHERE binding_id=? ORDER BY version').bind(id).all<Row>()).results,
 reads:async id=>(await write.prepare('SELECT binding_version,user_id,read_at,projection_version FROM customer_access_reads WHERE binding_id=? ORDER BY read_at DESC LIMIT 100').bind(id).all<Row>()).results,
}; }
function application(a:WorkContext) { return createCustomerAccessApplication({...ports(a),repository:async()=>repository(a,(await holdingResources(a.holdingId)).write)}); }
export function customerAccessBinding(a:WorkContext,id:string){return application(a).customerAccessBinding(id);}
export function customerAccessWorkspace(a:WorkContext){return application(a).customerAccessWorkspace();}
export function requestCustomerAccess(a:WorkContext,raw:unknown,key:string){return application(a).requestCustomerAccess(raw,key);}
export function changeCustomerAccess(a:WorkContext,id:string,raw:unknown,key:string){return application(a).changeCustomerAccess(id,raw,key);}
export function customerAccessHistory(a:WorkContext,id:string){return application(a).customerAccessHistory(id);}
export function customerAccessWork(a:WorkContext){return application(a).customerAccessWork();}
