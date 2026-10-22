import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Original immutable Files withdrawal witness; raw software withdrawal never
 * filters this source by purchase, current UI visibility, or the reader's role. */
export const messagingFileWithdrawalSources=`SELECT l.id link_id,s.id set_id,l.attachment_id source_id,s.company_id,s.target_id room_id,s.component_ref file_id,l.created_by,l.withdrawn_by,l.withdrawn_at,l.withdrawal_reason
 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
 JOIN attachments source ON source.id=l.attachment_id AND source.company_id=s.company_id
 WHERE s.target_type='messaging_room' AND s.component_kind='message' AND s.purpose_key='message_attachment' AND l.state='withdrawn'
 AND s.state='draft' AND l.created_by=s.created_by AND l.withdrawn_by=l.created_by AND l.withdrawn_at IS NOT NULL
 AND EXISTS(SELECT 1 FROM attachment_events event WHERE event.company_id=s.company_id AND event.link_id=l.id AND event.set_id=s.id AND event.attachment_id=l.attachment_id AND event.actor_id=l.withdrawn_by AND event.action='link.withdrawn' AND event.created_at=l.withdrawn_at AND event.details_json::jsonb->>'reason'=l.withdrawal_reason)`;
/** Generic Files can stage original conversation evidence before the native
 * message uploader creates its row. Keep that actual intake visible to raw
 * withdrawal even when it is absent from every Messages/Work projection. */
export const messagingFileIntakeSources=`SELECT l.id link_id,s.id set_id,s.company_id,s.target_id room_id,l.attachment_id source_id,l.created_by,l.state,
 jsonb_build_object('set',to_jsonb(s),'link',to_jsonb(l),'source',to_jsonb(source),'intent',to_jsonb(intent),'withdrawal',to_jsonb(withdrawn),'scans',(SELECT jsonb_agg(to_jsonb(scan) ORDER BY scan.id) FROM attachment_scan_attempts scan WHERE scan.attachment_id=source.id)) image,
 withdrawn.link_id IS NULL OR source.id IS NULL OR source.source_state<>'available' OR intent.id IS NULL OR intent.status NOT IN ('uploaded','ready') OR EXISTS(SELECT 1 FROM attachment_scan_attempts scan WHERE scan.attachment_id=source.id AND scan.state IN ('pending','running')) pending
 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
 LEFT JOIN attachments source ON source.id=l.attachment_id AND source.company_id=s.company_id
 LEFT JOIN upload_intents intent ON intent.id=source.upload_id AND intent.company_id=s.company_id AND intent.attachment_set_id=s.id AND intent.attachment_link_id=l.id
 LEFT JOIN (${messagingFileWithdrawalSources}) withdrawn ON withdrawn.link_id=l.id AND withdrawn.set_id=s.id AND withdrawn.source_id=source.id AND withdrawn.company_id=s.company_id AND withdrawn.room_id=s.target_id
 WHERE s.target_type='messaging_room'`;
export function messagingFileSettlementSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE attachment_links,attachment_sets,attachments,attachment_events,upload_intents,attachment_scan_attempts IN SHARE MODE');}
