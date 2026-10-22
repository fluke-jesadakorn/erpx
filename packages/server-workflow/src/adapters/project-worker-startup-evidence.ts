import 'server-only';
/** Fixed original owner evidence for one HR-only foundation placement. */
export const projectWorkerCommandEvidence={receipts:'SELECT command_id,operation,record_id,actor_id,authority_json,result_json FROM command_receipts',audit:'SELECT command_id,action,record_id,record_version FROM audit_events',outbox:'SELECT id,command_id,event_type,aggregate_id,aggregate_version,payload_json FROM outbox_events'} as const;
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export async function projectWorkerCommandEvidenceImage(database:SqlDatabase,companyId:string,recordId:string){return database.prepare(`SELECT jsonb_build_object(
 'receipts',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.command_id) FROM command_receipts r WHERE r.record_id=? AND r.authority_json::jsonb->>'companyId'=?),
 'audit',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM audit_events a JOIN command_receipts r ON r.command_id=a.command_id WHERE a.record_id=? AND r.authority_json::jsonb->>'companyId'=?),
 'outbox',(SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM outbox_events o WHERE o.aggregate_id=? AND o.payload_json::jsonb->>'companyId'=?))::text image`).bind(recordId,companyId,recordId,companyId,recordId,companyId).first<string>('image');}
