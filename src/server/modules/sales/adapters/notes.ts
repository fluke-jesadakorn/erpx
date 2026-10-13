import { commercialOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "../../finance/public/commercial-owner-projection-sources";
import {salesNoteCustomer} from "../../finance/public/sales-customer-sources";
import 'server-only';
import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
import {getOrder,type Row} from "@/server/modules/commerce/public/commerce";
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {requirePermission} from "@/server/modules/identity_access/public/iam-permissions";
import {hasPermission} from "@/lib/identity_access/permissions";
import {samePerson} from "@/lib/identity_access/iam";
import {assertion,executeCommand,replayCommand} from "@/server/modules/workflow/public/commands";
import {ApiError} from "@/server/modules/runtime/public/http";
import {customerTimeline} from "@/server/modules/customer_360/public/customer-timeline";

import {BusinessRuleError} from '../../shared';
import {createCustomerNoteApplication,type CustomerNoteRepository,type CustomerNoteMutation} from '../application/notes';
import type {ContactNote} from '../domain/notes';
export type {CustomerNotesView} from '../domain/notes';
export {customerNoteSchema,customerNoteChangeSchema} from '../domain/notes';
const scope="o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)";
const args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];

function application(a:WorkContext){let resource:Promise<Awaited<ReturnType<typeof holdingResources>>['write']>|null=null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);return createCustomerNoteApplication({canSell:hasPermission(a,'commerce.sales'),canManage:hasPermission(a,'commerce.approve'),branchId:a.branchId,departmentId:a.departmentId,authorize:permission=>{requirePermission(a,permission);},isSamePerson:id=>samePerson(a,id),clock:{now:()=>Date.now()},repository:async()=>repository(await database(),a)});}
function repository(write:Awaited<ReturnType<typeof holdingResources>>['write'],a:WorkContext):CustomerNoteRepository{return {
 order:async id=>(await getOrder(a,id)).order,timeline:async id=>{await customerTimeline(a,id,{limit:1});},
 customer:(id,version)=>salesNoteCustomer(write,a,id,version),
 async notes(kind,id){const orderId=id,customerId=id;return kind==='order'?(await write.prepare(`SELECT n.* FROM customer_contact_notes n JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.order_id=? AND ${scope} ORDER BY n.created_at DESC,n.id DESC`).bind(orderId,...args(a)).all<ContactNote>()).results:(await write.prepare(`SELECT n.* FROM customer_contact_notes n LEFT JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.company_id=? AND n.customer_id=? AND (n.order_id IS NULL AND ?::text IS NULL AND ?::text IS NULL OR n.order_id IS NOT NULL AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)) ORDER BY n.created_at DESC,n.id DESC`).bind(a.companyId,customerId,a.branchId,a.departmentId,a.branchId,a.branchId,a.departmentId,a.departmentId).all<ContactNote>()).results;},
 async history(kind,id){const orderId=id,customerId=id;return kind==='order'?(await write.prepare(`SELECT h.* FROM customer_contact_note_history h JOIN customer_contact_notes n ON n.id=h.note_id JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.order_id=? AND ${scope} ORDER BY h.created_at DESC,h.version DESC`).bind(orderId,...args(a)).all<Row>()).results:(await write.prepare(`SELECT h.* FROM customer_contact_note_history h JOIN customer_contact_notes n ON n.id=h.note_id LEFT JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.company_id=? AND n.customer_id=? AND (n.order_id IS NULL AND ?::text IS NULL AND ?::text IS NULL OR n.order_id IS NOT NULL AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)) ORDER BY h.created_at DESC,h.version DESC`).bind(a.companyId,customerId,a.branchId,a.departmentId,a.branchId,a.branchId,a.departmentId,a.departmentId).all<Row>()).results;},
 async note(id){return await write.prepare(`SELECT n.*,o.branch_id,o.department_id FROM customer_contact_notes n LEFT JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.id=? AND n.company_id=?`).bind(id,a.companyId).first<Row>();},
 replay:(key,operation,id,input)=>replayCommand(write,a,key,operation,id,input),commit:m=>commit(write,a,m),
};}
async function commit(write:Awaited<ReturnType<typeof holdingResources>>['write'],a:WorkContext,m:CustomerNoteMutation){const {key,id,operation,now}=m;switch(m.action){
 case 'create-order':{const input=m.input,order=m.record;return executeCommand(write,a,key,operation,input,id,1,{id,version:1},[
   assertion(write,key,'customer-source',`EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_orders}) o JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id AND p.company_id=o.company_id WHERE o.id=? AND o.version=? AND o.kind IN ('sales','quote') AND p.kind='customer' AND ${scope})`,[input.orderId!,input.expectedVersion,...args(a)]),
   write.prepare('INSERT INTO customer_contact_notes(id,company_id,customer_id,order_id,channel,occurred_at,body,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(id,a.companyId,order.party_id,input.orderId!,input.channel,input.occurredAt,input.body,a.userId,now,now),
   write.prepare("INSERT INTO customer_contact_note_history VALUES(?,?,1,'create',?,'Initial contact record',?,?)").bind(`contact-event-${key}`,id,input.body,a.userId,now)
  ],{companyId:a.companyId,branchId:String(order.branch_id),departmentId:order.department_id?String(order.department_id):null});}
 case 'create-customer':{const input=m.input,customerId=m.customerId;return executeCommand(write,a,key,operation,input,id,1,{id,version:1},[
  assertion(write,key,'customer-source',`EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE id=? AND company_id=? AND kind='customer' AND version=?)`,[customerId,a.companyId,input.expectedVersion]),
  write.prepare('INSERT INTO customer_contact_notes(id,company_id,customer_id,order_id,channel,occurred_at,body,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(id,a.companyId,customerId,null,input.channel,input.occurredAt,input.body,a.userId,now,now),
  write.prepare("INSERT INTO customer_contact_note_history VALUES(?,?,1,'create',?,'Initial standalone customer contact record',?,?)").bind(`contact-event-${key}`,id,input.body,a.userId,now)
 ],{companyId:a.companyId,branchId:null,departmentId:null});}
 case 'change':{const input=m.input,n=m.record,{content,status}=m;return executeCommand(write,a,key,operation,input,id,input.expectedVersion+1,{id,version:input.expectedVersion+1,status},[
 assertion(write,key,'note-version',"EXISTS(SELECT 1 FROM customer_contact_notes WHERE id=? AND company_id=? AND version=? AND status='active')",[id,a.companyId,input.expectedVersion]),
 write.prepare('UPDATE customer_contact_notes SET body=?,status=?,version=version+1,updated_at=? WHERE id=?').bind(content,status,now,id),
 write.prepare('INSERT INTO customer_contact_note_history VALUES(?,?,?,?,?,?,?,?)').bind(`contact-event-${key}`,id,input.expectedVersion+1,input.action,content,input.reason,a.userId,now)
 ],{companyId:a.companyId,branchId:String(n.branch_id),departmentId:n.department_id?String(n.department_id):null});}
}}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function customerNotesForOrder(a:WorkContext,id:string){return translated(()=>application(a).forOrder(id));}
export function customerNotesForCustomer(a:WorkContext,id:string){return translated(()=>application(a).forCustomer(id));}
export function createCustomerNote(a:WorkContext,raw:unknown,key:string){return translated(()=>application(a).create(raw,key));}
export function changeCustomerNote(a:WorkContext,id:string,raw:unknown,key:string){return translated(()=>application(a).change(id,raw,key));}

