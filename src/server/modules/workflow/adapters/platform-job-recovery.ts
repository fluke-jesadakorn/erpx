import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export type PlatformJobCounts = { pending_events: number|string; dead_letter_events: number|string; claimed_events: number|string; oldest_pending_at: number|string|null };
export type PlatformDeadLetterSource = { id:string;event_type:string;aggregate_id:string;aggregate_version:number;attempts:number;last_error:string|null;dead_letter_at:number };
export function platformJobRecoveryFacts(db:SqlDatabase){return {
 counts:(at:number)=>db.prepare(`SELECT COUNT(*) FILTER (WHERE processed_at IS NULL AND dead_letter_at IS NULL AND (claim_until IS NULL OR claim_until<=?)) pending_events, COUNT(*) FILTER (WHERE dead_letter_at IS NOT NULL) dead_letter_events, COUNT(*) FILTER (WHERE processed_at IS NULL AND dead_letter_at IS NULL AND claim_until>?) claimed_events, MIN(created_at) FILTER (WHERE processed_at IS NULL AND dead_letter_at IS NULL) oldest_pending_at FROM outbox_events`).bind(at,at).first<PlatformJobCounts>(),
 deadLetters:()=>db.prepare('SELECT id,event_type,aggregate_id,aggregate_version,attempts,last_error,dead_letter_at FROM outbox_events WHERE dead_letter_at IS NOT NULL ORDER BY dead_letter_at ASC,id ASC LIMIT 20').all<PlatformDeadLetterSource>(),
 receipt:(key:string)=>db.prepare("SELECT request_hash,result_json FROM command_receipts WHERE command_id=?").bind(key).first<{request_hash:string;result_json:string|null}>(),
 requeue:(now:number,id:string,attempts:number,lastError:string|null)=>db.prepare(`UPDATE outbox_events SET processed_at=NULL,published_at=NULL,claim_until=NULL,lease_token=NULL,
             next_attempt_at=?,last_error=NULL,dead_letter_at=NULL,attempts=0
           WHERE id=? AND dead_letter_at IS NOT NULL AND attempts=? AND last_error IS NOT DISTINCT FROM ?
           RETURNING id,aggregate_version`).bind(now,id,attempts,lastError).first<{id:string;aggregate_version:number}>(),
 recoveryJournal:(facts:{auditId:string;key:string;operation:string;actorId:string;assignmentId:string;digest:string;eventId:string;resultJson:string;now:number;receiptAuthorityJson:string;aggregateVersion:number;reason:string;auditAuthorityJson:string})=>[
  db.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?)").bind(facts.key,facts.operation,facts.actorId,facts.digest,facts.eventId,facts.resultJson,facts.now,facts.receiptAuthorityJson),
  db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(facts.auditId,facts.key,facts.actorId,facts.assignmentId,facts.operation,"runtime_job",facts.eventId,facts.aggregateVersion,facts.reason,facts.now,facts.auditAuthorityJson),
 ],
};}
