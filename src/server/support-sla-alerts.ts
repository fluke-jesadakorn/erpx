import 'server-only';
import {z} from 'zod';
import type {SqlDatabase} from './runtime/types';
import type {WorkContext} from './authorization';
import type {WorkItem} from '@/lib/work-item';
import {hasPermission} from '@/lib/permissions';
import {holdingResources} from './runtime';
import {requirePermission} from './iam-permissions';
import {assertion,executeCommand,replayCommand} from './commands';
import {supportRecipientEligibility} from './support-recipient';
import {ApiError} from './http';

// t is the source ticket; m is a current assignment, never a stored routing copy.
const recipient=`m.company_id=t.company_id AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_role_allows(m.role,m.company_id,'support.manage')
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id))
 AND ${supportRecipientEligibility().replaceAll('t.owner_id','m.user_id')}`;
const scope='t.company_id=? AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?)';
const args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
type Alert={id:string;ticket_id:string;title:string;source_version:number;breached_at:number;detected_at:number;version:number;acknowledged_by:string|null;acknowledged_at:number|null;reason:string|null;company_id:string;branch_id:string;department_id:string|null;can_ack:number;recipients:number};

/** Retrying after downtime or a partial sweep never duplicates a breach or recipient notice. */
export async function drainSupportSla(db:SqlDatabase,now=Date.now()) {
 await db.batch([
  db.prepare(`INSERT INTO support_sla_alerts(id,ticket_id,source_version,breached_at,detected_at)
   SELECT 'support-breach-'||t.id,t.id,t.version,
    COALESCE((t.sla_clock_json->>'breachedAt')::bigint,(t.sla_clock_json->>'dueAt')::bigint),?
   FROM support_tickets t WHERE (t.sla_clock_json->>'breachedAt')::bigint IS NOT NULL
     OR (t.sla_clock_json->>'state'='running' AND (t.sla_clock_json->>'dueAt')::bigint<?)
   ON CONFLICT DO NOTHING`).bind(now,now),
  db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
   SELECT DISTINCT s.id||':'||m.user_id,t.company_id,m.user_id,'support_sla_alert',s.id,s.version,
    'Resolution target breached: take responsibility for follow-up',?::bigint
   FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id JOIN assignments m ON ${recipient}
   WHERE s.acknowledged_by IS NULL ON CONFLICT DO NOTHING`).bind(now),
 ]);
}
export async function supportAlerts(a:WorkContext):Promise<Alert[]> {
 requirePermission(a,'support.read');const {write}=await holdingResources(a.holdingId);
 return (await write.prepare(`SELECT s.*,t.title,t.company_id,t.branch_id,t.department_id,
  CASE WHEN EXISTS(SELECT 1 FROM assignments m WHERE ${recipient} AND m.user_id=?) THEN 1 ELSE 0 END can_ack,
  (SELECT COUNT(DISTINCT m.user_id) FROM assignments m WHERE ${recipient}) recipients
  FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id WHERE ${scope} ORDER BY s.breached_at,s.id`).bind(a.userId,...args(a)).all<Alert>()).results;
}
export async function getSupportAlert(a:WorkContext,id:string) {
 const row=(await supportAlerts(a)).find(r=>r.id===id);
 if(!row)throw new ApiError(404,'SUPPORT_ALERT_NOT_FOUND','Target escalation is not available in this scope.');return row;
}
export async function acknowledgeSupportAlert(a:WorkContext,id:string,raw:unknown,key:string) {
 requirePermission(a,'support.manage');const input=z.object({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(2000)}).parse(raw);
 const {write}=await holdingResources(a.holdingId),op='support.sla.acknowledge',prior=await replayCommand(write,a,key,op,id,input);if(prior)return prior;
 const r=await getSupportAlert(a,id),now=Date.now();
 return executeCommand(write,a,key,op,input,id,input.expectedVersion+1,{id,version:input.expectedVersion+1},[
  assertion(write,key,'alert',`EXISTS(SELECT 1 FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id WHERE s.id=? AND s.version=? AND s.acknowledged_by IS NULL AND ${scope} AND EXISTS(SELECT 1 FROM assignments m WHERE ${recipient} AND m.user_id=?))`,[id,input.expectedVersion,...args(a),a.userId]),
  write.prepare('UPDATE support_sla_alerts SET acknowledged_by=?,acknowledged_at=?,reason=?,version=version+1 WHERE id=?').bind(a.userId,now,input.reason,id),
 ],{companyId:r.company_id,branchId:r.branch_id,departmentId:r.department_id});
}
export async function supportAlertWork(a:WorkContext):Promise<WorkItem[]> {
 if(!hasPermission(a,'support.read'))return [];
 return (await supportAlerts(a)).map(r=>{const done=!!r.acknowledged_by,canAct=!done&&hasPermission(a,'support.manage')&&Number(r.can_ack)>0;return {
  id:r.id,type:'support_sla_alert',title:`Target breached · ${r.title}`,detail:'Record who will follow up; the breach and ticket state are retained.',href:'/support/sla',
  status:done?'acknowledged':'open',updatedAt:Number(r.acknowledged_at??r.detected_at),dueAt:Number(r.breached_at),queue:done?'completed':canAct?'todo':'waiting',canAct,
  responsibleQueue:'Eligible Support manager',nextAction:done?'Follow the recorded recovery plan on the source case':'Take responsibility and record the follow-up plan',
  blocker:!done&&!Number(r.recipients)?'No eligible Support manager: administrator coverage is required':null,relationships:[canAct?'approval':'team'],
 };});
}
