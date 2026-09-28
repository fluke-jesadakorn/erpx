import 'server-only';
import {z} from 'zod';
import type {SqlDatabase} from './runtime/types';
import type {WorkContext} from './authorization';
import type {Row} from './commerce';
import {holdingResources} from './runtime';
import {getSupportTicket} from './support';
import {supportRecipientEligibility} from './support-recipient';
import {hasPermission} from '@/lib/permissions';
import {assertion,executeCommand,replayCommand} from './commands';
import {ApiError} from './http';
import {prepareAttachmentBinding} from './attachments/service';
import {attachmentsV2ReadOnly} from './attachments/feature';
const eligible=supportRecipientEligibility();
function requireSupportEvidenceWrite() {
 if(attachmentsV2ReadOnly())throw new ApiError(503,'ATTACHMENTS_READ_ONLY','Support evidence changes are disabled during attachment rollback. Existing clean bound evidence remains readable.');
}
export async function supportEvidenceGuard(db:SqlDatabase,a:WorkContext,id:string,expectedVersion:number,key:string) {
 const row=await getSupportTicket(a,id);
 if(!Number.isSafeInteger(expectedVersion)||expectedVersion<1)throw new ApiError(422,'VERSION_REQUIRED','Support evidence requires the current case version.');
 const manager=hasPermission(a,'support.manage');
 if(!manager&&!(hasPermission(a,'support.work')&&row.owner_id===a.userId))throw new ApiError(403,'SUPPORT_EVIDENCE_OWNER','Only the assigned colleague or a Support manager may change case evidence.');
 const actorEligible=await db.prepare(`SELECT 1 AS ok FROM support_tickets t WHERE t.id=? AND ${eligible.replaceAll('t.owner_id','?::text')}`).bind(id,a.userId).first();
 if(!actorEligible)throw new ApiError(409,'SUPPORT_EVIDENCE_ASSIGNMENT','Your current employment and assignment no longer permit changes to this case evidence.');
 if(!['open','in_progress','waiting_customer'].includes(String(row.status)))throw new ApiError(409,'SUPPORT_EVIDENCE_LOCKED','Evidence is frozen once a resolution is proposed. Return the case for rework before changing it.');
 return {row,guards:[assertion(db,key,'support-evidence',`EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.company_id=? AND t.version=? AND t.status IN ('open','in_progress','waiting_customer') AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND ${manager?eligible.replaceAll('t.owner_id','?::text'):eligible+' AND t.owner_id=?'})`,[id,a.companyId,expectedVersion,a.branchId,a.branchId,a.departmentId,a.departmentId,a.userId])]};
}
export function supportEvidenceEvent(db:SqlDatabase,id:string,key:string,actor:string,action:string,evidence:Record<string,unknown>,reason:string) {
 return db.prepare('INSERT INTO support_ticket_events SELECT ?,id,version,?,?,?,owner_id,due_date,status,updated_at,?::jsonb FROM support_tickets WHERE id=?').bind(`support-evidence-${key}`,action,actor,reason,JSON.stringify(evidence),id);
}
export async function supportEvidence(a:WorkContext,id:string) {
 const row=await getSupportTicket(a,id),{write}=await holdingResources(a.holdingId);
 const items=(await write.prepare(`SELECT a.id,a.original_name,a.media_type,a.bytes,a.checksum_sha256,a.created_at,a.created_by,a.revoked_at,a.legal_hold
   FROM attachments a WHERE a.owner_type='support_ticket' AND a.owner_id=? AND (a.company_id IS NULL OR EXISTS(
     SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
     WHERE l.attachment_id=a.id AND l.state='bound' AND s.company_id=a.company_id AND s.target_type='support_ticket' AND s.target_id=a.owner_id))
   ORDER BY a.created_at,a.id`).bind(id).all<Row>()).results;
 const actorEligible=await write.prepare(`SELECT 1 AS ok FROM support_tickets t WHERE t.id=? AND ${eligible.replaceAll('t.owner_id','?::text')}`).bind(id,a.userId).first();
 const mutable=!attachmentsV2ReadOnly()&&Boolean(actorEligible)&&['open','in_progress','waiting_customer'].includes(String(row.status));
 return {
  items,
  canChange:mutable&&(hasPermission(a,'support.manage')||(hasPermission(a,'support.work')&&row.owner_id===a.userId&&Number(row.owner_eligible)>0)),
  canShare:mutable&&hasPermission(a,'support.manage'),
 };
}
export async function bindSupportEvidence(a:WorkContext,id:string,raw:unknown,key:string) {
 const input=z.object({
 expectedVersion:z.number().int().positive(),
  attachmentSetIds:z.array(z.string().min(1)).min(1).max(1),
  reason:z.string().trim().min(3).max(2000),
 }).parse(raw),{write}=await holdingResources(a.holdingId),op='support.evidence.bind',prior=await replayCommand(write,a,key,op,id,input);if(prior)return prior;requireSupportEvidenceWrite();
 const row=await getSupportTicket(a,id),prepared=await prepareAttachmentBinding(write,a,{
  attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:'support_ticket',bindingType:'support_action',contexts:['internal_evidence','customer_shareable'].map(purposeKey=>({purposeKey,componentKind:'action' as const,componentRef:`support-evidence:${purposeKey}`}))},targetId:id,
  expectedTargetVersion:input.expectedVersion,commandId:key,
  bindingId:`support-evidence:${key}`,
 }),now=Date.now(),result={id,version:input.expectedVersion+1,manifest:prepared.manifest};
 return executeCommand(write,a,key,op,input,id,input.expectedVersion+1,result,[
  ...prepared.guards,
  write.prepare('UPDATE support_tickets SET version=version+1,updated_at=? WHERE id=? AND version=?').bind(now,id,input.expectedVersion),
  ...prepared.steps,
  supportEvidenceEvent(write,id,key,a.userId,'attachment-bind',{manifest:prepared.manifest},input.reason),
 ],{companyId:a.companyId,branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null});
}
export async function revokeSupportEvidence(a:WorkContext,id:string,raw:unknown,key:string) {
 const input=z.object({expectedVersion:z.number().int().positive(),attachmentId:z.string().min(1),reason:z.string().trim().min(3).max(2000)}).parse(raw),{write}=await holdingResources(a.holdingId),op='support.evidence.revoke',prior=await replayCommand(write,a,key,op,id,input);if(prior)return prior;requireSupportEvidenceWrite();
 const {row,guards}=await supportEvidenceGuard(write,a,id,input.expectedVersion,key),now=Date.now();
 return executeCommand(write,a,key,op,input,id,input.expectedVersion+1,{id,version:input.expectedVersion+1},[
  ...guards,assertion(write,key,'attachment',"EXISTS(SELECT 1 FROM attachments WHERE id=? AND owner_type='support_ticket' AND owner_id=? AND revoked_at IS NULL AND legal_hold=0)",[input.attachmentId,id]),
  write.prepare('UPDATE attachments SET revoked_at=? WHERE id=?').bind(now,input.attachmentId),
  write.prepare('UPDATE support_tickets SET version=version+1,updated_at=? WHERE id=?').bind(now,id),
  supportEvidenceEvent(write,id,key,a.userId,'attachment-revoke',input,input.reason),
 ],{companyId:a.companyId,branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null});
}
