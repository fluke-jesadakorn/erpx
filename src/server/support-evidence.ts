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
const eligible=supportRecipientEligibility();
export async function supportEvidenceGuard(db:SqlDatabase,a:WorkContext,id:string,expectedVersion:number,key:string) {
 const row=await getSupportTicket(a,id);
 if(!Number.isSafeInteger(expectedVersion)||expectedVersion<1)throw new ApiError(422,'VERSION_REQUIRED','Support evidence requires the current case version.');
 const manager=hasPermission(a,'support.manage');
 if(!manager&&!(hasPermission(a,'support.work')&&row.owner_id===a.userId))throw new ApiError(403,'SUPPORT_EVIDENCE_OWNER','Only the assigned colleague or a Support manager may change case evidence.');
 if(!['open','in_progress','waiting_customer'].includes(String(row.status)))throw new ApiError(409,'SUPPORT_EVIDENCE_LOCKED','Evidence is frozen once a resolution is proposed. Return the case for rework before changing it.');
 return {row,guards:[assertion(db,key,'support-evidence',`EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=? AND t.company_id=? AND t.version=? AND t.status IN ('open','in_progress','waiting_customer') AND (?::text IS NULL OR t.branch_id=?) AND (?::text IS NULL OR t.department_id=?) AND ${manager?eligible.replaceAll('t.owner_id','?::text'):eligible+' AND t.owner_id=?'})`,[id,a.companyId,expectedVersion,a.branchId,a.branchId,a.departmentId,a.departmentId,a.userId])]};
}
export function supportEvidenceEvent(db:SqlDatabase,id:string,key:string,actor:string,action:string,evidence:Record<string,unknown>,reason:string) {
 return db.prepare('INSERT INTO support_ticket_events SELECT ?,id,version,?,?,?,owner_id,due_date,status,updated_at,?::jsonb FROM support_tickets WHERE id=?').bind(`support-evidence-${key}`,action,actor,reason,JSON.stringify(evidence),id);
}
export async function supportEvidence(a:WorkContext,id:string) {
 const row=await getSupportTicket(a,id),{write}=await holdingResources(a.holdingId);
 const items=(await write.prepare("SELECT id,original_name,media_type,bytes,checksum_sha256,created_at,created_by,revoked_at,legal_hold FROM attachments WHERE owner_type='support_ticket' AND owner_id=? ORDER BY created_at,id").bind(id).all<Row>()).results;
 const actorEligible=await write.prepare(`SELECT 1 AS ok FROM support_tickets t WHERE t.id=? AND ${eligible.replaceAll('t.owner_id','?::text')}`).bind(id,a.userId).first();
 return {items,canChange:Boolean(actorEligible)&&['open','in_progress','waiting_customer'].includes(String(row.status))&&(hasPermission(a,'support.manage')||(hasPermission(a,'support.work')&&row.owner_id===a.userId&&Number(row.owner_eligible)>0))};
}
export async function revokeSupportEvidence(a:WorkContext,id:string,raw:unknown,key:string) {
 const input=z.object({expectedVersion:z.number().int().positive(),attachmentId:z.string().min(1),reason:z.string().trim().min(3).max(2000)}).parse(raw),{write}=await holdingResources(a.holdingId),op='support.evidence.revoke',prior=await replayCommand(write,a,key,op,id,input);if(prior)return prior;
 const {row,guards}=await supportEvidenceGuard(write,a,id,input.expectedVersion,key),now=Date.now();
 return executeCommand(write,a,key,op,input,id,input.expectedVersion+1,{id,version:input.expectedVersion+1},[
  ...guards,assertion(write,key,'attachment',"EXISTS(SELECT 1 FROM attachments WHERE id=? AND owner_type='support_ticket' AND owner_id=? AND revoked_at IS NULL AND legal_hold=0)",[input.attachmentId,id]),
  write.prepare('UPDATE attachments SET revoked_at=? WHERE id=?').bind(now,input.attachmentId),
  write.prepare('UPDATE support_tickets SET version=version+1,updated_at=? WHERE id=?').bind(now,id),
  supportEvidenceEvent(write,id,key,a.userId,'attachment-revoke',input,input.reason),
 ],{companyId:a.companyId,branchId:String(row.branch_id),departmentId:row.department_id?String(row.department_id):null});
}
