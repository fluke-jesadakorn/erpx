import 'server-only';
import {z} from 'zod';
import type {WorkContext} from './authorization';
import type {WorkItem} from '@/lib/work-item';
import type {Row} from './commerce';
import type {SqlDatabase} from './runtime/types';
import {holdingResources} from './runtime';
import {requirePermission} from './iam-permissions';
import {hasPermission} from '@/lib/permissions';
import {samePerson} from '@/lib/iam';
import {ApiError} from './http';
import {assertion,executeCommand,replayCommand} from './commands';
import {getSupportTicket,supportRecipients} from './support';
import {supportRecipientEligibility} from './support-recipient';

const id=z.string().min(1),version=z.number().int().positive(),reason=z.string().trim().min(3).max(4000);
const scope='t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)';
const args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
const eligible=supportRecipientEligibility();
const reviewer=`m.company_id=t.company_id AND m.user_id<>t.owner_id AND m.user_id IS DISTINCT FROM t.mitigated_by
 AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage')
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))
 AND ${eligible.replaceAll('t.owner_id','m.user_id')}`;
export const incidentCreateSchema=z.object({ticketId:id,ticketVersion:version,title:z.string().trim().min(3).max(160),impact:reason,severity:z.enum(['critical','high','normal']),ownerId:id,dueDate:z.iso.date()});
export const incidentChangeSchema=z.discriminatedUnion('action',[
 z.object({action:z.enum(['start','update','reject','close','reopen','cancel']),expectedVersion:version,reason}),
 z.object({action:z.literal('mitigate'),expectedVersion:version,reason,rootCause:reason,mitigation:reason}),
 z.object({action:z.literal('reassign'),expectedVersion:version,reason,ownerId:id,dueDate:z.iso.date()}),
 z.object({action:z.literal('unlink'),expectedVersion:version,reason,ticketId:id}),
 z.object({action:z.literal('link'),expectedVersion:version,reason,ticketId:id,ticketVersion:version}),
]);
const terminal=(s:unknown)=>['closed','cancelled'].includes(String(s));
export async function getSupportIncident(a:WorkContext,id:string) {
 requirePermission(a,'support.read');const {write}=await holdingResources(a.holdingId);
 const r=await write.prepare(`SELECT t.*,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible,
  CASE WHEN EXISTS(SELECT 1 FROM assignments m WHERE ${reviewer} AND m.user_id=?) THEN 1 ELSE 0 END reviewer_eligible,
  (SELECT COUNT(DISTINCT m.user_id) FROM assignments m WHERE ${reviewer}) reviewer_count
  FROM support_incidents t WHERE t.id=? AND ${scope}`).bind(a.userId,id,...args(a)).first<Row>();
 if(!r)throw new ApiError(404,'SUPPORT_INCIDENT_NOT_FOUND','Incident is not available in this scope.');return r;
}
function notify(db:SqlDatabase,id:string,key:string) {
 return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
  SELECT DISTINCT ?||u.user_id,t.company_id,u.user_id,'support_incident',t.id,t.version,'Incident updated: review impact and follow up each linked customer case',t.updated_at
  FROM support_incidents t JOIN LATERAL(SELECT t.owner_id user_id UNION SELECT s.owner_id FROM support_incident_tickets l JOIN support_tickets s ON s.id=l.ticket_id WHERE l.incident_id=t.id AND l.active UNION SELECT m.user_id FROM assignments m WHERE t.status='mitigated' AND ${reviewer}) u ON true
  WHERE t.id=? ON CONFLICT DO NOTHING`).bind(`incident-notice-${key}-`,id);
}
export async function createSupportIncident(a:WorkContext,raw:unknown,key:string) {
 requirePermission(a,'support.manage');const input=incidentCreateSchema.parse(raw),{write}=await holdingResources(a.holdingId),id=`incident-${key}`,op='support.incident.create',prior=await replayCommand(write,a,key,op,id,input);if(prior)return prior;
 const source=await getSupportTicket(a,input.ticketId),now=Date.now();
 return executeCommand(write,a,key,op,input,id,1,{id,version:1},[
  assertion(write,key,'source',`EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.version=? AND ${scope})`,[input.ticketId,input.ticketVersion,...args(a)]),
  write.prepare('INSERT INTO support_incidents(id,company_id,branch_id,department_id,title,impact,severity,owner_id,due_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(id,a.companyId,source.branch_id,source.department_id,input.title,input.impact,input.severity,input.ownerId,input.dueDate,a.userId,now,now),
  assertion(write,key,'owner',`EXISTS(SELECT 1 FROM support_incidents t WHERE t.id=? AND ${eligible})`,[id]),
  write.prepare('INSERT INTO support_incident_tickets(incident_id,ticket_id,linked_version,linked_by,linked_at) VALUES(?,?,?,?,?)').bind(id,input.ticketId,input.ticketVersion,a.userId,now),
  write.prepare("INSERT INTO support_incident_events VALUES(?,?,1,'create',?,?,?,?::jsonb)").bind(`incident-event-${key}`,id,a.userId,input.impact,now,JSON.stringify(input)),notify(write,id,key),
 ],{companyId:a.companyId,branchId:String(source.branch_id),departmentId:source.department_id?String(source.department_id):null});
}
export async function changeSupportIncident(a:WorkContext,id:string,raw:unknown,key:string) {
 const input=incidentChangeSchema.parse(raw),manage=['close','reject','reopen','cancel','reassign','link','unlink'].includes(input.action);requirePermission(a,manage?'support.manage':'support.work');
 const {write}=await holdingResources(a.holdingId),op=`support.incident.${input.action}`,prior=await replayCommand(write,a,key,op,id,input);if(prior)return prior;
 const r=await getSupportIncident(a,id),now=Date.now(),allowed:Record<string,string[]>={start:['open'],update:['open','investigating'],mitigate:['investigating'],reject:['mitigated'],close:['mitigated'],reopen:['closed'],cancel:['open','investigating','mitigated'],reassign:['open','investigating','mitigated'],link:['open','investigating'],unlink:['open','investigating']};
 if(!allowed[input.action].includes(String(r.status)))throw new ApiError(409,'SUPPORT_INCIDENT_STATE','Review the current incident state.');
 if(!manage&&!samePerson(a,r.owner_id))throw new ApiError(403,'SUPPORT_INCIDENT_OWNER_REQUIRED','The assigned incident owner must perform this action.');
 if(input.action==='close'&&(samePerson(a,r.owner_id)||samePerson(a,r.mitigated_by)))throw new ApiError(403,'INDEPENDENT_APPROVER_REQUIRED','Another manager must review the incident mitigation.');
 const status=({start:'investigating',mitigate:'mitigated',reject:'investigating',close:'closed',reopen:'investigating',cancel:'cancelled'} as Record<string,string>)[input.action]??r.status;
 const steps=[assertion(write,key,'version',`EXISTS(SELECT 1 FROM support_incidents t WHERE t.id=? AND t.version=? AND t.status=? AND ${scope})`,[id,input.expectedVersion,r.status,...args(a)])];
 if(!manage)steps.push(assertion(write,key,'owner',`EXISTS(SELECT 1 FROM support_incidents t WHERE t.id=? AND ${eligible})`,[id]));
 if(input.action==='close')steps.push(assertion(write,key,'reviewer',`EXISTS(SELECT 1 FROM support_incidents t JOIN assignments m ON ${reviewer} WHERE t.id=? AND m.user_id=?)`,[id,a.userId]));
 if(input.action==='link') {
  await getSupportTicket(a,input.ticketId);
  steps.push(assertion(write,key,'linked-source',`EXISTS(SELECT 1 FROM support_tickets s JOIN support_incidents t ON s.company_id=t.company_id AND s.branch_id=t.branch_id AND s.department_id IS NOT DISTINCT FROM t.department_id WHERE t.id=? AND s.id=? AND s.version=?)`,[id,input.ticketId,input.ticketVersion]),assertion(write,key,'not-linked','NOT EXISTS(SELECT 1 FROM support_incident_tickets WHERE incident_id=? AND ticket_id=? AND active)',[id,input.ticketId]),write.prepare('INSERT INTO support_incident_tickets(incident_id,ticket_id,linked_version,linked_by,linked_at) VALUES(?,?,?,?,?) ON CONFLICT(incident_id,ticket_id) DO UPDATE SET linked_version=excluded.linked_version,linked_by=excluded.linked_by,linked_at=excluded.linked_at,active=true').bind(id,input.ticketId,input.ticketVersion,a.userId,now));
 }
 if(input.action==='unlink')steps.push(assertion(write,key,'linked','EXISTS(SELECT 1 FROM support_incident_tickets WHERE incident_id=? AND ticket_id=? AND active) AND (SELECT COUNT(*) FROM support_incident_tickets WHERE incident_id=? AND active)>1',[id,input.ticketId,id]),write.prepare('UPDATE support_incident_tickets SET active=false WHERE incident_id=? AND ticket_id=?').bind(id,input.ticketId));
 steps.push(write.prepare('UPDATE support_incidents SET status=?,owner_id=?,due_date=?,root_cause=?,mitigation=?,mitigated_by=?,version=version+1,updated_at=? WHERE id=?').bind(status,input.action==='reassign'?input.ownerId:r.owner_id,input.action==='reassign'?input.dueDate:r.due_date,input.action==='mitigate'?input.rootCause:r.root_cause,input.action==='mitigate'?input.mitigation:r.mitigation,input.action==='mitigate'?a.userId:r.mitigated_by,now,id));
 if(['reassign','reopen','reject'].includes(input.action))steps.push(assertion(write,key,'new-owner',`EXISTS(SELECT 1 FROM support_incidents t WHERE t.id=? AND ${eligible})`,[id]));
 steps.push(write.prepare('INSERT INTO support_incident_events VALUES(?,?,?,?,?,?,?,?::jsonb)').bind(`incident-event-${key}`,id,input.expectedVersion+1,input.action,a.userId,input.reason,now,JSON.stringify({...input,status})),notify(write,id,key));
 return executeCommand(write,a,key,op,input,id,input.expectedVersion+1,{id,version:input.expectedVersion+1,status},steps,{companyId:a.companyId,branchId:String(r.branch_id),departmentId:r.department_id?String(r.department_id):null});
}
export async function incidentWorkspace(a:WorkContext) {
 requirePermission(a,'support.read');const {write}=await holdingResources(a.holdingId);
 const items=(await write.prepare(`SELECT t.* FROM support_incidents t WHERE ${scope} ORDER BY t.updated_at DESC,t.id LIMIT 200`).bind(...args(a)).all<Row>()).results;
 const tickets=(await write.prepare(`SELECT t.id,t.title,t.version,t.status FROM support_tickets t WHERE ${scope} ORDER BY t.updated_at DESC,t.id LIMIT 200`).bind(...args(a)).all<Row>()).results;
 return {items,tickets,canManage:hasPermission(a,'support.manage')};
}
export async function incidentDetail(a:WorkContext,id:string) {
 const row=await getSupportIncident(a,id),{write}=await holdingResources(a.holdingId),links=(await write.prepare('SELECT t.id,t.title,t.status,t.version,l.linked_version FROM support_incident_tickets l JOIN support_tickets t ON t.id=l.ticket_id WHERE l.incident_id=? AND l.active ORDER BY l.linked_at,t.id').bind(id).all<Row>()).results;
 const first=await getSupportTicket(a,String(links[0].id));
 return {row,links,recipients:await supportRecipients(a,String(first.order_id)),candidates:(await write.prepare('SELECT id,title,version FROM support_tickets WHERE company_id=? AND branch_id=? AND department_id IS NOT DISTINCT FROM ?::text AND id NOT IN(SELECT ticket_id FROM support_incident_tickets WHERE incident_id=? AND active) ORDER BY updated_at DESC LIMIT 200').bind(a.companyId,row.branch_id,row.department_id,id).all<Row>()).results,
  history:(await write.prepare('SELECT * FROM support_incident_events WHERE incident_id=? ORDER BY version DESC').bind(id).all<Row>()).results,
  canManage:hasPermission(a,'support.manage'),canWork:hasPermission(a,'support.work')&&samePerson(a,row.owner_id)&&Number(row.owner_eligible)>0,canReview:hasPermission(a,'support.manage')&&Number(row.reviewer_eligible)>0};
}
export async function incidentWork(a:WorkContext):Promise<WorkItem[]> {
 if(!hasPermission(a,'support.read'))return [];const {write}=await holdingResources(a.holdingId);
 const rows=(await write.prepare(`SELECT t.*,CASE WHEN ${eligible} THEN 1 ELSE 0 END owner_eligible,CASE WHEN EXISTS(SELECT 1 FROM assignments m WHERE ${reviewer} AND m.user_id=?) THEN 1 ELSE 0 END reviewer_eligible,(SELECT COUNT(DISTINCT m.user_id) FROM assignments m WHERE ${reviewer}) reviewer_count FROM support_incidents t WHERE ${scope}`).bind(a.userId,...args(a)).all<Row>()).results;
 return rows.map(r=>{const done=terminal(r.status),review=r.status==='mitigated',canAct=!done&&(review?hasPermission(a,'support.manage')&&Number(r.reviewer_eligible)>0:hasPermission(a,'support.work')&&samePerson(a,r.owner_id)&&Number(r.owner_eligible)>0);return {id:String(r.id),type:'support_incident',title:`${r.severity} incident · ${r.title}`,detail:'Shared cause; each linked customer case retains its own outcome.',href:`/support/incidents/${r.id}`,status:String(r.status),updatedAt:Number(r.updated_at),queue:done?'completed':canAct?'todo':'waiting',canAct,dueAt:Date.parse(String(r.due_date)+'T00:00:00Z'),responsibleQueue:review?'Independent Support manager':'Assigned incident owner',nextAction:review?'Review mitigation or request rework':'Investigate and record mitigation evidence',blocker:!done&&(review?!Number(r.reviewer_count):!Number(r.owner_eligible))?'No eligible recipient; Support manager or administrator coverage is required':null,relationships:[samePerson(a,r.owner_id)?'assigned':samePerson(a,r.created_by)?'requested':canAct?'approval':'team']};});
}
