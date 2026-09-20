import 'server-only';
import {z} from 'zod';
import type {WorkContext} from './authorization';
import {getOrder,type Row} from './commerce';
import {holdingResources} from './runtime';
import {requirePermission} from './iam-permissions';
import {hasPermission} from '@/lib/permissions';
import {assertion,executeCommand,replayCommand} from './commands';
import {ApiError} from './http';
import {salesRecipientEligibility} from './sales-recipient';
import type {WorkItem} from '@/lib/work-item';
type AccessRow=Record<string,unknown>&{id:string;number:string;email:string;status:string;expires_at:number;version:number;created_by:string;approved_by:string|null;reviewed_at:number|null;order_id:string;branch_id:string;department_id:string|null;updated_at:number;reviewers?:number;canReview:boolean;canRevoke:boolean};
const reason=z.string().trim().min(3).max(2000),version=z.number().int().positive();
export const customerAccessRequestSchema=z.object({orderId:z.string().min(1),sourceVersion:version,email:z.email().max(254).transform(s=>s.toLowerCase()),expiresAt:z.number().int().positive(),reason});
const changeSchema=z.object({action:z.enum(['approve','reject','revoke']),expectedVersion:version,reason});
const scope="o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)";
const args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
const eligible=salesRecipientEligibility().replaceAll('c.owner_id','m.user_id');
const reviewers=`m.company_id=o.company_id AND m.user_id<>b.created_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'commerce.approve') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=o.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=o.department_id)) AND ${eligible}`;
const approvedSource="o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')";
export async function customerAccessBinding(a:WorkContext,id:string){
 requirePermission(a,'commerce.read');requirePermission(a,'commerce.sales');const {write}=await holdingResources(a.holdingId);
 const row=await write.prepare(`SELECT b.*,o.number,o.branch_id,o.department_id,CASE WHEN EXISTS(SELECT 1 FROM assignments m WHERE ${reviewers} AND m.user_id=?) THEN 1 ELSE 0 END actor_eligible FROM customer_access_bindings b JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id WHERE b.id=? AND ${scope}`).bind(a.userId,id,...args(a)).first<Row>();
 if(!row)throw new ApiError(404,'CUSTOMER_ACCESS_NOT_FOUND','Customer access request not found in this scope.');return {...row,canReview:row.status==='pending'&&Number(row.actor_eligible)>0&&hasPermission(a,'commerce.approve'),canRevoke:['pending','approved'].includes(String(row.status))&&(row.created_by===a.userId||hasPermission(a,'commerce.approve'))} as AccessRow;
}
export async function customerAccessWorkspace(a:WorkContext){
 requirePermission(a,'commerce.read');requirePermission(a,'commerce.sales');const {write}=await holdingResources(a.holdingId);
 const rows=(await write.prepare(`SELECT b.*,o.number,CASE WHEN EXISTS(SELECT 1 FROM assignments m WHERE ${reviewers} AND m.user_id=?) THEN 1 ELSE 0 END actor_eligible,(SELECT COUNT(DISTINCT m.user_id) FROM assignments m WHERE ${reviewers}) reviewers FROM customer_access_bindings b JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id WHERE ${scope} ORDER BY b.updated_at DESC,b.id`).bind(a.userId,...args(a)).all<Row>()).results;
 const sources=(await write.prepare(`SELECT o.id,o.number,o.version,p.name FROM commerce_orders o JOIN finance_parties p ON p.id=o.party_id AND p.company_id=o.company_id WHERE ${scope} AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled') ORDER BY o.updated_at DESC LIMIT 250`).bind(...args(a)).all<Row>()).results;
 const mapped:AccessRow[]=rows.map(r=>({...r,canReview:r.status==='pending'&&Number(r.actor_eligible)>0&&hasPermission(a,'commerce.approve'),canRevoke:['pending','approved'].includes(String(r.status))&&(r.created_by===a.userId||hasPermission(a,'commerce.approve'))}) as AccessRow);
 return {rows:mapped,sources};
}
export async function requestCustomerAccess(a:WorkContext,raw:unknown,key:string){
 requirePermission(a,'commerce.sales');requirePermission(a,'commerce.read');const input=customerAccessRequestSchema.parse(raw),{write}=await holdingResources(a.holdingId),id=`customer-access-${key}`,op='customer.access.request',prior=await replayCommand(write,a,key,op,id,input);if(prior)return prior;
 const now=Date.now();if(input.expiresAt<=now||input.expiresAt>now+90*86400000)throw new ApiError(422,'CUSTOMER_ACCESS_EXPIRY','Choose an expiry within the next 90 days.');const {order}=await getOrder(a,input.orderId);
 return executeCommand(write,a,key,op,input,id,1,{id,version:1,status:'pending'},[
 assertion(write,key,'source',`EXISTS(SELECT 1 FROM commerce_orders o WHERE o.id=? AND o.version=? AND ${scope} AND o.kind IN ('sales','quote') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled'))`,[input.orderId,input.sourceVersion,...args(a)]),
 write.prepare('INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,reason,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(id,a.companyId,input.orderId,order.party_id,input.email,input.expiresAt,input.reason,a.userId,now,now),
 write.prepare("INSERT INTO customer_access_events VALUES(?,?,1,'request',?,?,?)").bind(`customer-access-event-${key}`,id,a.userId,input.reason,now),
 write.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT DISTINCT ?||m.user_id,b.company_id,m.user_id,'customer_access',b.id,b.version,'Review a source-bound customer access request',b.updated_at FROM customer_access_bindings b JOIN commerce_orders o ON o.id=b.order_id JOIN assignments m ON ${reviewers} WHERE b.id=? ON CONFLICT DO NOTHING`).bind(`customer-access-notice-${key}-`,id)
 ],{companyId:a.companyId,branchId:String(order.branch_id),departmentId:order.department_id?String(order.department_id):null});
}
export async function changeCustomerAccess(a:WorkContext,id:string,raw:unknown,key:string){
 requirePermission(a,'commerce.sales');requirePermission(a,'commerce.read');const input=changeSchema.parse(raw),{write}=await holdingResources(a.holdingId),op=`customer.access.${input.action}`,prior=await replayCommand(write,a,key,op,id,input);if(prior)return prior;
 const row=await customerAccessBinding(a,id),review=input.action!=='revoke';
 if(review){requirePermission(a,'commerce.approve');if(row.created_by===a.userId)throw new ApiError(403,'INDEPENDENT_APPROVER_REQUIRED','Another authorized sales manager must review customer access.');}else if(row.created_by!==a.userId&&!hasPermission(a,'commerce.approve'))throw new ApiError(403,'FORBIDDEN','Only the requester or a sales manager may revoke this access.');
 const now=Date.now(),status=input.action==='approve'?'approved':input.action==='reject'?'rejected':'revoked';
 return executeCommand(write,a,key,op,input,id,input.expectedVersion+1,{id,version:input.expectedVersion+1,status},[
 assertion(write,key,'binding',`EXISTS(SELECT 1 FROM customer_access_bindings b JOIN commerce_orders o ON o.id=b.order_id WHERE b.id=? AND b.version=? AND ${scope} AND b.status IN (${review?"'pending'":"'pending','approved'"}) ${review?'AND b.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint':''})`,[id,input.expectedVersion,...args(a)]),
 ...(review?[assertion(write,key,'reviewer',`EXISTS(SELECT 1 FROM customer_access_bindings b JOIN commerce_orders o ON o.id=b.order_id JOIN assignments m ON ${reviewers} WHERE b.id=? AND m.user_id=?)`,[id,a.userId])]:[]),
 ...(input.action==='approve'?[assertion(write,key,'publishable-source',`EXISTS(SELECT 1 FROM commerce_orders o WHERE o.id=? AND ${approvedSource})`,[String(row.order_id)])]:[]),
 write.prepare('UPDATE customer_access_bindings SET status=?,approved_by=?,reviewed_at=?,reason=?,version=version+1,updated_at=? WHERE id=?').bind(status,input.action==='approve'?a.userId:row.approved_by,review?now:row.reviewed_at,input.reason,now,id),
 write.prepare('INSERT INTO customer_access_events VALUES(?,?,?,?,?,?,?)').bind(`customer-access-event-${key}`,id,input.expectedVersion+1,input.action,a.userId,input.reason,now),
 write.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,created_by,'customer_access',id,version,'Customer access decision recorded: review the source',updated_at FROM customer_access_bindings WHERE id=?").bind(`customer-access-notice-${key}`,id)
 ],{companyId:a.companyId,branchId:String(row.branch_id),departmentId:row.department_id?String(row.department_id):null});
}
export async function customerAccessHistory(a:WorkContext,id:string){const binding=await customerAccessBinding(a,id),{write}=await holdingResources(a.holdingId);return {binding,events:(await write.prepare('SELECT version,action,actor_id,reason,created_at FROM customer_access_events WHERE binding_id=? ORDER BY version').bind(id).all<Row>()).results,reads:(await write.prepare('SELECT binding_version,user_id,read_at,projection_version FROM customer_access_reads WHERE binding_id=? ORDER BY read_at DESC LIMIT 100').bind(id).all<Row>()).results};}
export async function customerAccessWork(a:WorkContext):Promise<WorkItem[]>{if(!hasPermission(a,'commerce.sales')||!hasPermission(a,'commerce.read'))return [];const {rows}=await customerAccessWorkspace(a);return rows.map(r=>{const pending=r.status==='pending'&&Number(r.expires_at)>Date.now();return {id:String(r.id),type:'customer_access',title:`Customer access · ${r.number}`,detail:'Verify the recipient and authority evidence before sharing this source.',href:`/customer-access/${encodeURIComponent(String(r.id))}`,status:Number(r.expires_at)<=Date.now()?'expired':String(r.status),updatedAt:Number(r.updated_at),queue:pending?r.canReview?'todo':'waiting':'completed',canAct:pending&&r.canReview,responsibleQueue:'Independent sales manager',nextAction:pending?'Review or revoke the access request':'Read retained access evidence',blocker:pending&&!Number(r.reviewers)?'No eligible independent sales manager; administrator coverage is required':null,relationships:[r.created_by===a.userId?'requested':r.canReview?'approval':'team']};});}
