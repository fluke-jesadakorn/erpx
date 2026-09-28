import 'server-only';
import {z} from 'zod';
import type {WorkContext} from './authorization';
import type {Row} from './commerce';
import type {WorkItem} from '@/lib/work-item';
import {holdingResources} from './runtime';
import {requirePermission} from './iam-permissions';
import {hasPermission} from '@/lib/permissions';
import {samePerson} from '@/lib/iam';
import {advanceSupportClock,type SupportClock} from '@/lib/support-clock';
import {ApiError} from './http';
import {assertion,executeCommand,replayCommand} from './commands';
import {supportRecipientEligibility} from './support-recipient';
const reason=z.string().trim().min(3).max(4000),version=z.number().int().positive(),id=z.string().min(1),scope="t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)",args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId],eligible=supportRecipientEligibility();
export const supportCreateSchema=z.union([
 z.object({kind:z.literal('general'),title:z.string().trim().min(3).max(160),issue:reason}),
 z.object({orderId:id,sourceVersion:version,title:z.string().trim().min(3).max(160),issue:reason,ownerId:id,dueDate:z.iso.date()}),
]);
export const supportChangeSchema=z.discriminatedUnion('action',[
 z.object({action:z.enum(['start','wait-customer','resume','resolve','reject-resolution','reopen','cancel']),expectedVersion:version,reason}),
 z.object({action:z.literal('reassign'),expectedVersion:version,reason,ownerId:id,dueDate:z.iso.date()}),
 z.object({action:z.literal('triage'),expectedVersion:version,reason,ownerId:id,dueDate:z.iso.date()}),
 z.object({action:z.literal('confirm-general'),expectedVersion:version,reason}),
 z.object({action:z.literal('confirm'),expectedVersion:version,reason,noteId:id,noteVersion:version})
]);
const reviewers=`(SELECT COUNT(DISTINCT m.user_id) FROM assignments m WHERE m.company_id=t.company_id AND m.user_id<>t.owner_id AND m.user_id IS DISTINCT FROM t.resolved_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND ${eligible.replaceAll('t.owner_id','m.user_id')})`;
const closed=(status:unknown)=>['closed','cancelled'].includes(String(status));
export async function getSupportTicket(a:WorkContext,id:string){requirePermission(a,'support.read');const {write}=await holdingResources(a.holdingId),r=await write.prepare(`SELECT t.*,o.number order_number,p.name customer_name,(SELECT legal_name FROM hr_people WHERE company_id=t.company_id AND user_id=t.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible,${reviewers} reviewer_count FROM support_tickets t LEFT JOIN commerce_orders o ON o.id=t.order_id AND o.company_id=t.company_id LEFT JOIN finance_parties p ON p.id=t.customer_id AND p.company_id=t.company_id WHERE t.id=? AND ${scope}`).bind(id,...args(a)).first<Row>();if(!r)throw new ApiError(404,'SUPPORT_TICKET_NOT_FOUND','Ticket not found in this scope.');return r;}
export async function supportWorkspace(a:WorkContext){requirePermission(a,'support.read');const {write}=await holdingResources(a.holdingId),items=(await write.prepare(`SELECT t.id,t.title,t.status,t.due_date,t.owner_id,t.version,t.updated_at FROM support_tickets t WHERE ${scope} ORDER BY t.updated_at DESC,t.id LIMIT 200`).bind(...args(a)).all<Row>()).results;
 const sources=hasPermission(a,'support.create')?(await write.prepare(`SELECT t.id,t.number,t.version,p.name customer_name FROM commerce_orders t JOIN finance_parties p ON p.id=t.party_id WHERE ${scope} AND t.kind IN ('sales','quote') AND p.kind='customer' ORDER BY t.updated_at DESC LIMIT 200`).bind(...args(a)).all<Row>()).results:[];
 return {items,sources,canCreate:hasPermission(a,'support.create')};}
export async function supportRecipients(a:WorkContext,orderId:string){requirePermission(a,'support.read');const {write}=await holdingResources(a.holdingId);return (await write.prepare(`SELECT DISTINCT t.owner_id id,p.legal_name name FROM(SELECT o.company_id,o.branch_id,o.department_id,a.user_id owner_id FROM commerce_orders o JOIN assignments a ON a.company_id=o.company_id WHERE o.id=?) t JOIN hr_people p ON p.company_id=t.company_id AND p.user_id=t.owner_id WHERE ${scope} AND ${eligible} ORDER BY p.legal_name`).bind(orderId,...args(a)).all<{id:string;name:string}>()).results;}
export async function supportTicketRecipients(a:WorkContext,ticketId:string){requirePermission(a,'support.read');const {write}=await holdingResources(a.holdingId);return (await write.prepare(`SELECT DISTINCT m.user_id id,p.legal_name name FROM support_tickets t JOIN assignments m ON m.company_id=t.company_id JOIN hr_people p ON p.company_id=t.company_id AND p.user_id=m.user_id WHERE t.id=? AND ${scope} AND ${eligible.replaceAll('t.owner_id','m.user_id')} ORDER BY p.legal_name`).bind(ticketId,...args(a)).all<{id:string;name:string}>()).results;}
export async function supportDetail(a:WorkContext,id:string){const row=await getSupportTicket(a,id),{write}=await holdingResources(a.holdingId);return {incidents:(await write.prepare('SELECT i.id,i.title,i.status FROM support_incident_tickets l JOIN support_incidents i ON i.id=l.incident_id WHERE l.ticket_id=? AND l.active ORDER BY i.updated_at DESC').bind(id).all<Row>()).results,now:Date.now(),row,recipients:await supportTicketRecipients(a,id),history:(await write.prepare('SELECT * FROM support_ticket_events WHERE ticket_id=? ORDER BY version DESC').bind(id).all<Row>()).results,notes:hasPermission(a,'support.manage')&&row.order_id?(await write.prepare("SELECT id,version,channel,occurred_at,body FROM customer_contact_notes WHERE order_id=? AND company_id=? AND status='active' ORDER BY occurred_at DESC").bind(row.order_id,a.companyId).all<Row>()).results:[],canWork:hasPermission(a,'support.work')&&row.owner_id===a.userId&&Number(row.owner_eligible)>0,canManage:hasPermission(a,'support.manage'),canConfirm:hasPermission(a,'support.manage')&&!samePerson(a,row.owner_id)&&!samePerson(a,row.resolved_by),canReadOrder:!!row.order_id&&hasPermission(a,'commerce.read'),slaClock:row.sla_clock_json?JSON.parse(String(row.sla_clock_json)) as SupportClock:null,slaPolicies:hasPermission(a,'support.manage')&&row.status==='open'&&!row.sla_clock_json?(await write.prepare("SELECT id,name,version FROM support_sla_policies WHERE company_id=? AND status='approved' ORDER BY name").bind(a.companyId).all<{id:string;name:string;version:number}>()).results:[]};}
export async function createSupportTicket(a:WorkContext,raw:unknown,key:string){
 const input=supportCreateSchema.parse(raw);requirePermission(a,'support.create');requirePermission(a,'support.read');
 const {write}=await holdingResources(a.holdingId),id=`ticket-${key}`,op='support.create',prior=await replayCommand(write,a,key,op,id,input);
 if(prior)return prior;
 const now=Date.now();
 if('kind' in input)return executeCommand(write,a,key,op,input,id,1,{id,version:1,status:'triage'},[
  write.prepare("INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,source_kind,created_by,created_at,updated_at) VALUES(?,?,?,?,NULL,NULL,?,?,NULL,NULL,'triage','general',?,?,?)")
   .bind(id,a.companyId,a.branchId,a.departmentId,input.title,input.issue,a.userId,now,now),
  write.prepare("INSERT INTO support_ticket_events SELECT ?,id,version,'report',?,?,owner_id,due_date,status,updated_at,?::jsonb FROM support_tickets WHERE id=?")
   .bind(`ticket-event-${key}`,a.userId,input.issue,JSON.stringify(input),id),
  write.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
    SELECT DISTINCT ?||m.user_id,t.company_id,m.user_id,'support_ticket',t.id,t.version,'New issue needs Support triage',t.updated_at
    FROM support_tickets t JOIN assignments m ON m.company_id=t.company_id
    WHERE t.id=? AND m.user_id<>t.created_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_role_allows(m.role,m.company_id,'support.manage')
      AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))
      AND ${eligible.replaceAll('t.owner_id','m.user_id')} ON CONFLICT DO NOTHING`).bind(`ticket-triage-${key}-`,id),
 ],{companyId:a.companyId,branchId:a.branchId,departmentId:a.departmentId});
 return executeCommand(write,a,key,op,input,id,1,{id,version:1},[
  assertion(write,key,'source',`EXISTS(SELECT 1 FROM commerce_orders t JOIN finance_parties p ON p.id=t.party_id AND p.company_id=t.company_id WHERE t.id=? AND t.version=? AND t.kind IN ('sales','quote') AND p.kind='customer' AND ${scope})`,[input.orderId,input.sourceVersion,...args(a)]),
  write.prepare('INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at) SELECT ?,company_id,branch_id,department_id,id,party_id,?,?,?,?,?,?,? FROM commerce_orders WHERE id=?').bind(id,input.title,input.issue,input.ownerId,input.dueDate,a.userId,now,now,input.orderId),
  assertion(write,key,'owner',`EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND ${eligible})`,[id]),
  write.prepare("INSERT INTO support_ticket_events SELECT ?,id,version,'create',?,?,owner_id,due_date,status,updated_at,?::jsonb FROM support_tickets WHERE id=?").bind(`ticket-event-${key}`,a.userId,input.issue,JSON.stringify(input),id),
  write.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'support_ticket',id,version,'New customer support case: accept and review the issue',updated_at FROM support_tickets WHERE id=?").bind(`ticket-notice-${key}`,id),
 ]);
}
export async function changeSupportTicket(a:WorkContext,id:string,raw:unknown,key:string){
 const input=supportChangeSchema.parse(raw);requirePermission(a,'support.read');
 const manage=['triage','reassign','confirm','confirm-general','reject-resolution','reopen','cancel'].includes(input.action);
 requirePermission(a,manage?'support.manage':'support.work');
 const {write}=await holdingResources(a.holdingId),op=`support.${input.action}`,prior=await replayCommand(write,a,key,op,id,input);
 if(prior)return prior;
 const r=await getSupportTicket(a,id),now=Date.now();
 const allowed:Record<string,string[]>={triage:['triage'],start:['open'],'wait-customer':['in_progress'],resume:['waiting_customer'],resolve:['in_progress'],'reject-resolution':['resolved'],confirm:['resolved'],'confirm-general':['resolved'],reopen:['closed'],cancel:['triage','open','in_progress','waiting_customer','resolved'],reassign:['open','in_progress','waiting_customer','resolved']};
 if(!allowed[input.action].includes(String(r.status)))throw new ApiError(409,'SUPPORT_STATE_CHANGED','Review the current ticket state.');
 if(input.action==='confirm'&&r.source_kind!=='sales_order'||input.action==='confirm-general'&&r.source_kind!=='general')throw new ApiError(409,'SUPPORT_CONFIRMATION_MISMATCH','Choose the confirmation method for this issue source.');
 if(!manage&&r.owner_id!==a.userId)throw new ApiError(403,'SUPPORT_OWNER_REQUIRED','The assigned support colleague must perform this action.');
 if(['confirm','confirm-general'].includes(input.action)&&(samePerson(a,r.owner_id)||samePerson(a,r.resolved_by)))throw new ApiError(403,'INDEPENDENT_CONFIRMATION_REQUIRED','Another support manager must review this resolution.');
 const status=({triage:'open',start:'in_progress','wait-customer':'waiting_customer',resume:'in_progress',resolve:'resolved','reject-resolution':'in_progress',confirm:'closed','confirm-general':'closed',reopen:'in_progress',cancel:'cancelled'} as Record<string,string>)[input.action]??String(r.status);
 const steps=[assertion(write,key,'version',`EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.version=? AND t.status=? AND ${scope})`,[id,input.expectedVersion,r.status,...args(a)])];
 if(!manage)steps.push(assertion(write,key,'owner',`EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.owner_id=? AND ${eligible})`,[id,a.userId]));
 if(input.action==='confirm')steps.push(assertion(write,key,'customer-confirmation',"EXISTS(SELECT 1 FROM customer_contact_notes WHERE id=? AND version=? AND order_id=? AND company_id=? AND customer_id=? AND status='active' AND occurred_at>=? AND created_at>=?)",[input.noteId,input.noteVersion,r.order_id,a.companyId,r.customer_id,r.resolved_at,r.resolved_at]));
 const resetting=['reopen','reject-resolution'].includes(input.action);
 steps.push(write.prepare('UPDATE support_tickets SET status=?,owner_id=?,due_date=?,blocked_reason=?,resolution=?,resolved_by=?,resolved_at=?,confirmation_note_id=?,confirmation_note_version=?,confirmed_by=?,confirmed_at=?,confirmation_reason=?,version=version+1,updated_at=? WHERE id=?').bind(
  status,'ownerId' in input?input.ownerId:r.owner_id,'dueDate' in input?input.dueDate:r.due_date,
  status==='waiting_customer'?(input.action==='reassign'?r.blocked_reason:input.reason):null,
  input.action==='resolve'?input.reason:resetting?null:r.resolution,
  input.action==='resolve'?a.userId:resetting?null:r.resolved_by,
  input.action==='resolve'?now:resetting?null:r.resolved_at,
  input.action==='confirm'?input.noteId:input.action==='reopen'?null:r.confirmation_note_id,
  input.action==='confirm'?input.noteVersion:input.action==='reopen'?null:r.confirmation_note_version,
  input.action==='confirm-general'?a.userId:input.action==='reopen'?null:r.confirmed_by,
  input.action==='confirm-general'?now:input.action==='reopen'?null:r.confirmed_at,
  input.action==='confirm-general'?input.reason:input.action==='reopen'?null:r.confirmation_reason,
  now,id));
 if(['triage','reassign','reopen','reject-resolution'].includes(input.action))steps.push(assertion(write,key,'recipient',`EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND ${eligible})`,[id]));
 const nextClock=r.sla_clock_json?advanceSupportClock(JSON.parse(String(r.sla_clock_json)) as SupportClock,input.action,now):null;
 if(nextClock)steps.push(write.prepare('UPDATE support_tickets SET sla_clock_json=?::jsonb WHERE id=?').bind(JSON.stringify(nextClock),id));
 steps.push(write.prepare('INSERT INTO support_ticket_events SELECT ?,id,version,?,?,?,owner_id,due_date,status,updated_at,?::jsonb FROM support_tickets WHERE id=?').bind(`ticket-event-${key}`,input.action,a.userId,input.reason,JSON.stringify({...input,...(nextClock?{clock:nextClock}:{})}),id));
 if(input.action!=='cancel')steps.push(write.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'support_ticket',id,version,'Support case updated: review current status and next action',updated_at FROM support_tickets WHERE id=? AND owner_id IS NOT NULL").bind(`ticket-notice-${key}`,id));
 if(input.action==='resolve')steps.push(write.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT DISTINCT ?||m.user_id,t.company_id,m.user_id,'support_ticket',t.id,t.version,'Review proposed issue resolution',t.updated_at FROM support_tickets t JOIN assignments m ON m.company_id=t.company_id WHERE t.id=? AND m.user_id<>t.owner_id AND m.user_id<>t.resolved_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND ${eligible.replaceAll('t.owner_id','m.user_id')} ON CONFLICT DO NOTHING`).bind(`ticket-review-${key}-`,id));
 return executeCommand(write,a,key,op,input,id,input.expectedVersion+1,{id,status,version:input.expectedVersion+1},steps,{companyId:a.companyId,branchId:r.branch_id?String(r.branch_id):null,departmentId:r.department_id?String(r.department_id):null});
}
export async function supportWork(a:WorkContext):Promise<WorkItem[]>{
 if(!hasPermission(a,'support.read'))return [];
 const {write}=await holdingResources(a.holdingId),rows=(await write.prepare(`SELECT t.*,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible,${reviewers} reviewer_count FROM support_tickets t WHERE ${scope} ORDER BY t.updated_at DESC`).bind(...args(a)).all<Row>()).results;
 return rows.map(r=>{
  const c=r.sla_clock_json?JSON.parse(String(r.sla_clock_json)) as SupportClock:null,terminal=closed(r.status),triage=r.status==='triage',review=r.status==='resolved';
  const canAct=!terminal&&(triage?hasPermission(a,'support.manage'):review?hasPermission(a,'support.manage')&&r.owner_id!==a.userId&&!samePerson(a,r.resolved_by):r.owner_id===a.userId&&hasPermission(a,'support.work')&&Number(r.owner_eligible)>0);
  return {id:String(r.id),type:'support_ticket',title:String(r.title),detail:r.source_kind==='general'?'Issue reported for Support triage':'Customer support case',href:`/support/${r.id}`,status:String(r.status),updatedAt:Number(r.updated_at),queue:terminal?'completed':canAct?'todo':'waiting',canAct,responsibleQueue:triage?'Support triage':review?'Independent Support manager':'Assigned support colleague',nextAction:triage?'Choose an owner and follow-up date':review?r.source_kind==='general'?'Confirm outcome or return for further work':'Review customer confirmation or reject the proposed resolution':r.status==='waiting_customer'?'Follow up for information':'Accept and work the issue',dueAt:c?.state==='running'&&c.dueAt!==null?c.dueAt:r.due_date?Date.parse(String(r.due_date)+'T00:00:00Z'):null,blocker:review&&!Number(r.reviewer_count)?'No independent eligible Support manager; an administrator must assign authorized coverage':!triage&&!terminal&&!Number(r.owner_eligible)?'Assigned support colleague is no longer eligible; manager reassignment is required':r.blocked_reason?String(r.blocked_reason):null,relationships:[r.owner_id===a.userId?'assigned':r.created_by===a.userId?'requested':'team']};
 });
}
