import 'server-only';
import {z} from 'zod';
import type {WorkContext} from './authorization';
import {getOrder,type Row} from './commerce';
import {holdingResources} from './runtime';
import {requirePermission} from './iam-permissions';
import {hasPermission} from '@/lib/permissions';
import {samePerson} from '@/lib/iam';
import {assertion,executeCommand,replayCommand} from './commands';
import {ApiError} from './http';
import {customerTimeline} from './customer-timeline';
type ContactNote={id:string;company_id:string;customer_id:string;order_id:string|null;channel:string;occurred_at:number;body:string;status:string;version:number;created_by:string;created_at:number;updated_at:number};
export type CustomerNotesView={customer:{id:string;name:string;code:string;version:number};order?:{id:string;party_id:string;party_name:string;number:string;version:number};notes:(ContactNote&{canChange:boolean})[];history:Row[];canCreate:boolean};
const body=z.string().trim().min(3).max(4000),version=z.number().int().positive();
export const customerNoteSchema=z.object({orderId:z.string().min(1).optional(),customerId:z.string().min(1).optional(),expectedVersion:version,channel:z.enum(['phone','email','meeting','other']),occurredAt:z.number().int().nonnegative(),body}).superRefine((value,ctx)=>{if(Boolean(value.orderId)===Boolean(value.customerId))ctx.addIssue({code:'custom',path:['source'],message:'Choose exactly one order or customer source.'});});
export const customerNoteChangeSchema=z.discriminatedUnion('action',[z.object({action:z.literal('correct'),expectedVersion:version,body,reason:body}),z.object({action:z.literal('archive'),expectedVersion:version,reason:body})]);
const scope="o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)";
const args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
const canChange=(a:WorkContext,n:Row)=>hasPermission(a,'commerce.sales')&&(samePerson(a,n.created_by)||hasPermission(a,'commerce.approve'));
export async function customerNotesForOrder(a:WorkContext,orderId:string){
 requirePermission(a,'commerce.read');const source=await getOrder(a,orderId);if(source.order.kind==='purchase')throw new ApiError(422,'CUSTOMER_SOURCE_REQUIRED','Use a customer sales order or quotation.');
 const {write}=await holdingResources(a.holdingId);const notes=(await write.prepare(`SELECT n.* FROM customer_contact_notes n JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.order_id=? AND ${scope} ORDER BY n.created_at DESC,n.id DESC`).bind(orderId,...args(a)).all<ContactNote>()).results;
 const history=(await write.prepare(`SELECT h.* FROM customer_contact_note_history h JOIN customer_contact_notes n ON n.id=h.note_id JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.order_id=? AND ${scope} ORDER BY h.created_at DESC,h.version DESC`).bind(orderId,...args(a)).all<Row>()).results;
 const order={id:String(source.order.id),party_id:String(source.order.party_id),party_name:String(source.order.party_name),number:String(source.order.number),version:Number(source.order.version)};
 return {customer:{id:String(source.order.party_id),name:String(source.order.party_name),code:String(source.order.party_code??''),version:Number(source.order.party_version??1)},order,notes:notes.map(n=>({...n,canChange:n.status==='active'&&canChange(a,n)})),history,canCreate:hasPermission(a,'commerce.sales')} satisfies CustomerNotesView;
}
export async function customerNotesForCustomer(a:WorkContext,customerId:string):Promise<CustomerNotesView>{
 requirePermission(a,'commerce.read');
 const {write}=await holdingResources(a.holdingId);
 await customerTimeline(a,customerId,{limit:1});
 const customer=await write.prepare("SELECT p.id,p.name,p.code,p.version FROM finance_parties p WHERE p.id=? AND p.company_id=? AND p.kind='customer'").bind(customerId,a.companyId).first<{id:string;name:string;code:string;version:number}>();
 if(!customer)throw new ApiError(404,'CUSTOMER_NOT_FOUND','Customer is not available in this workspace.');
 const notes=(await write.prepare(`SELECT n.* FROM customer_contact_notes n LEFT JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.company_id=? AND n.customer_id=? AND (n.order_id IS NULL AND ?::text IS NULL AND ?::text IS NULL OR n.order_id IS NOT NULL AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)) ORDER BY n.created_at DESC,n.id DESC`).bind(a.companyId,customerId,a.branchId,a.departmentId,a.branchId,a.branchId,a.departmentId,a.departmentId).all<ContactNote>()).results;
 const history=(await write.prepare(`SELECT h.* FROM customer_contact_note_history h JOIN customer_contact_notes n ON n.id=h.note_id LEFT JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.company_id=? AND n.customer_id=? AND (n.order_id IS NULL AND ?::text IS NULL AND ?::text IS NULL OR n.order_id IS NOT NULL AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)) ORDER BY h.created_at DESC,h.version DESC`).bind(a.companyId,customerId,a.branchId,a.departmentId,a.branchId,a.branchId,a.departmentId,a.departmentId).all<Row>()).results;
 return {customer,notes:notes.map(n=>({...n,canChange:n.status==='active'&&canChange(a,n)})),history,canCreate:hasPermission(a,'commerce.sales')};
}
export async function createCustomerNote(a:WorkContext,raw:unknown,key:string){
 const input=customerNoteSchema.parse(raw);requirePermission(a,'commerce.sales');requirePermission(a,'commerce.read');
 if(input.occurredAt>Date.now()+60000)throw new ApiError(422,'CONTACT_TIME_INVALID','Record a contact that already happened.');
 const {write}=await holdingResources(a.holdingId),id=`contact-${key}`,operation='customer.note.create';const prior=await replayCommand(write,a,key,operation,id,input);if(prior)return prior;
 const now=Date.now();
 if(input.orderId){
  const {order}=await getOrder(a,input.orderId);if(order.kind==='purchase')throw new ApiError(422,'CUSTOMER_SOURCE_REQUIRED','Use a customer sales order or quotation.');
  return executeCommand(write,a,key,operation,input,id,1,{id,version:1},[
   assertion(write,key,'customer-source',`EXISTS(SELECT 1 FROM commerce_orders o JOIN finance_parties p ON p.id=o.party_id AND p.company_id=o.company_id WHERE o.id=? AND o.version=? AND o.kind IN ('sales','quote') AND p.kind='customer' AND ${scope})`,[input.orderId,input.expectedVersion,...args(a)]),
   write.prepare('INSERT INTO customer_contact_notes(id,company_id,customer_id,order_id,channel,occurred_at,body,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(id,a.companyId,order.party_id,input.orderId,input.channel,input.occurredAt,input.body,a.userId,now,now),
   write.prepare("INSERT INTO customer_contact_note_history VALUES(?,?,1,'create',?,'Initial contact record',?,?)").bind(`contact-event-${key}`,id,input.body,a.userId,now)
  ],{companyId:a.companyId,branchId:String(order.branch_id),departmentId:order.department_id?String(order.department_id):null});
 }
 const customerId=String(input.customerId);
 await customerTimeline(a,customerId,{limit:1});
 const customer=await write.prepare("SELECT id,version FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND version=?").bind(customerId,a.companyId,input.expectedVersion).first<{id:string;version:number}>();
 if(!customer)throw new ApiError(409,'CUSTOMER_SOURCE_CHANGED','Refresh the Customer 360 record before recording a note.');
 return executeCommand(write,a,key,operation,input,id,1,{id,version:1},[
  assertion(write,key,'customer-source',"EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND version=?)",[customerId,a.companyId,input.expectedVersion]),
  write.prepare('INSERT INTO customer_contact_notes(id,company_id,customer_id,order_id,channel,occurred_at,body,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(id,a.companyId,customerId,null,input.channel,input.occurredAt,input.body,a.userId,now,now),
  write.prepare("INSERT INTO customer_contact_note_history VALUES(?,?,1,'create',?,'Initial standalone customer contact record',?,?)").bind(`contact-event-${key}`,id,input.body,a.userId,now)
 ],{companyId:a.companyId,branchId:null,departmentId:null});
}
export async function changeCustomerNote(a:WorkContext,id:string,raw:unknown,key:string){
 const input=customerNoteChangeSchema.parse(raw);requirePermission(a,'commerce.sales');requirePermission(a,'commerce.read');const {write}=await holdingResources(a.holdingId),operation=`customer.note.${input.action}`;const prior=await replayCommand(write,a,key,operation,id,input);if(prior)return prior;
 const n=await write.prepare('SELECT n.*,o.branch_id,o.department_id FROM customer_contact_notes n LEFT JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.id=? AND n.company_id=?').bind(id,a.companyId).first<Row>();if(!n)throw new ApiError(404,'CUSTOMER_NOTE_NOT_FOUND','Contact note not found in this scope.');
 const inScope=n.order_id===null ? a.branchId===null&&a.departmentId===null : (a.branchId===null||n.branch_id===a.branchId)&&(a.departmentId===null||n.department_id===a.departmentId);if(!inScope)throw new ApiError(404,'CUSTOMER_NOTE_NOT_FOUND','Contact note not found in this scope.');
 if(n.order_id===null){await customerTimeline(a,String(n.customer_id),{limit:1});}
 if(!canChange(a,n))throw new ApiError(403,'CUSTOMER_NOTE_OWNER_REQUIRED','The author or a sales manager must correct this contact record.');
 const content=input.action==='correct'?input.body:n.body,status=input.action==='archive'?'archived':'active',now=Date.now();
 return executeCommand(write,a,key,operation,input,id,input.expectedVersion+1,{id,version:input.expectedVersion+1,status},[
 assertion(write,key,'note-version',"EXISTS(SELECT 1 FROM customer_contact_notes WHERE id=? AND company_id=? AND version=? AND status='active')",[id,a.companyId,input.expectedVersion]),
 write.prepare('UPDATE customer_contact_notes SET body=?,status=?,version=version+1,updated_at=? WHERE id=?').bind(content,status,now,id),
 write.prepare('INSERT INTO customer_contact_note_history VALUES(?,?,?,?,?,?,?,?)').bind(`contact-event-${key}`,id,input.expectedVersion+1,input.action,content,input.reason,a.userId,now)
 ],{companyId:a.companyId,branchId:String(n.branch_id),departmentId:n.department_id?String(n.department_id):null});
}
