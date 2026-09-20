import 'server-only';
import {z} from 'zod';
import type {WorkContext} from './authorization';
import {holdingResources} from './runtime';
import {requirePermission} from './iam-permissions';
import {ApiError} from './http';
import {hasPermission} from '@/lib/permissions';
const cursorSchema=z.object({at:z.number().int().nonnegative(),id:z.string().min(1).max(250)});
const querySchema=z.object({cursor:z.string().max(1000).optional(),limit:z.coerce.number().int().min(1).max(100).default(30)});
function visible(a:WorkContext){return {sql:`WITH scoped_orders AS (SELECT id,party_id,kind,number,state,version,created_at,updated_at FROM commerce_orders WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) AND kind IN ('sales','quote')),scoped_leads AS (SELECT l.id,l.title,l.status,l.version,l.created_at,l.updated_at,v.customer_id,v.stage FROM crm_leads l JOIN crm_opportunities v ON v.lead_id=l.id WHERE l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?))`,args:[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId,a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId]};}
export async function customerDirectory(a:WorkContext){requirePermission(a,'commerce.read');const {write}=await holdingResources(a.holdingId),s=visible(a);return (await write.prepare(`${s.sql} SELECT p.id,p.name,p.code FROM finance_parties p WHERE p.company_id=? AND p.kind='customer' AND (EXISTS(SELECT 1 FROM scoped_orders o WHERE o.party_id=p.id) OR EXISTS(SELECT 1 FROM scoped_leads l WHERE l.customer_id=p.id)) ORDER BY p.name,p.id LIMIT 201`).bind(...s.args,a.companyId).all<{id:string;name:string;code:string}>()).results;}
export type CustomerTimelineItem={id:string;source_id:string;source_type:string;title:string;status:string;version:number;at:number;href:string};
export async function customerTimeline(a:WorkContext,customerId:string,raw:unknown={}){
 requirePermission(a,'commerce.read');const q=querySchema.parse(raw);let cursor:z.infer<typeof cursorSchema>|null=null;
 if(q.cursor){try{cursor=cursorSchema.parse(JSON.parse(Buffer.from(q.cursor,'base64url').toString('utf8')));}catch{throw new ApiError(422,'TIMELINE_CURSOR_INVALID','Use a cursor returned by this customer timeline.');}}
 const {write}=await holdingResources(a.holdingId),s=visible(a),customer=await write.prepare(`${s.sql} SELECT p.id,p.name,p.code FROM finance_parties p WHERE p.id=? AND p.company_id=? AND p.kind='customer' AND (EXISTS(SELECT 1 FROM scoped_orders o WHERE o.party_id=p.id) OR EXISTS(SELECT 1 FROM scoped_leads l WHERE l.customer_id=p.id))`).bind(...s.args,customerId,a.companyId).first<{id:string;name:string;code:string}>();
 if(!customer)throw new ApiError(404,'CUSTOMER_TIMELINE_NOT_FOUND','Customer not found in this scope.');
 // Only these display fields leave this projection. Internal notes, contact details, costs,
 // HR, Legal, payment beneficiaries and event payloads stay with their source owners.
 const rows=(await write.prepare(`${s.sql}, orders AS(SELECT * FROM scoped_orders WHERE party_id=?),leads AS(SELECT * FROM scoped_leads WHERE customer_id=?), events AS (
 SELECT 'order:'||o.id id,o.id source_id,'order' source_type,o.kind||' · '||o.number title,o.state status,o.version,o.updated_at at,'/commerce/orders/'||o.id href FROM orders o
 UNION ALL SELECT 'fulfillment:'||f.id,f.id,'fulfillment','Fulfillment · '||f.number,'recorded',f.version,f.created_at,'/commerce/orders/'||o.id FROM commerce_fulfillments f JOIN orders o ON o.id=f.order_id
 UNION ALL SELECT 'return:'||r.id,r.id,'return','Customer return · '||r.number,r.status,r.version,r.updated_at,'/commerce/orders/'||o.id FROM commerce_returns r JOIN orders o ON o.id=r.order_id WHERE r.company_id=? AND r.kind='customer_return'
 UNION ALL SELECT 'contact:'||h.id,n.id,'contact','Contact · '||n.channel,h.action,h.version,h.created_at,'/commerce/orders/'||o.id||'/notes' FROM customer_contact_note_history h JOIN customer_contact_notes n ON n.id=h.note_id JOIN orders o ON o.id=n.order_id WHERE n.company_id=?
 UNION ALL SELECT 'followup:'||e.id,c.id,'followup','Customer follow-up',e.action,e.version,e.created_at,'/customer-followups/'||c.id FROM customer_followup_events e JOIN customer_followups c ON c.id=e.followup_id JOIN orders o ON o.id=c.order_id WHERE c.company_id=?
 UNION ALL SELECT 'support:'||e.id,t.id,'support',t.title,e.action,e.version,e.created_at,'/support/'||t.id FROM support_ticket_events e JOIN support_tickets t ON t.id=e.ticket_id JOIN orders o ON o.id=t.order_id WHERE t.company_id=? AND ?=1
 UNION ALL SELECT 'crm:'||e.id,l.id,'crm',l.title,e.action,e.version,e.created_at,'/crm/'||l.id FROM crm_case_events e JOIN leads l ON l.id=e.lead_id WHERE e.opportunity_id IS NULL
 ) SELECT id,source_id,source_type,title,status,version,at,href FROM events WHERE (?::bigint IS NULL OR at<? OR (at=? AND id<?)) ORDER BY at DESC,id DESC LIMIT ?`).bind(...s.args,customerId,customerId,a.companyId,a.companyId,a.companyId,a.companyId,hasPermission(a,'support.read')?1:0,cursor?.at??null,cursor?.at??null,cursor?.at??null,cursor?.id??null,q.limit+1).all<CustomerTimelineItem>()).results;
 const items=rows.slice(0,q.limit),last=items.at(-1),hasMore=rows.length>q.limit;return {customer,items,hasMore,nextCursor:hasMore&&last?Buffer.from(JSON.stringify({at:Number(last.at),id:last.id})).toString('base64url'):null,coverage:['Sales quotations/orders','Fulfillments and customer returns','Internal contact history','Customer follow-up history','Qualified CRM lead/opportunity history',...(hasPermission(a,'support.read')?['Support case history']:[])],limitations:'Internal staff projection. Source records remain authoritative. Order/return entries show their latest state, not a complete transition log. Field Service, Legal and customer Portal are not included.'};
}
