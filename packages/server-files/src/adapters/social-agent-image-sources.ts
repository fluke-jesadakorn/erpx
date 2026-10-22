import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {socialDraftAttachmentJoin} from '@erpx/server-collaboration/public/social-draft-file-projection';
/** Closed original Social AI source association. Only native draft images
 * published on this exact post/comment may select a private storage key. */
export async function socialAgentImageSource(database:SqlDatabase,input:{id:string;companyId:string;postId:string;commentId:string|null}){
 return database.prepare(`SELECT a.object_key,a.checksum_sha256,a.media_type,a.bytes,
 jsonb_build_object('attachment',to_jsonb(a),'draft',to_jsonb(d),'intent',to_jsonb(u),
 'scans',(SELECT jsonb_agg(to_jsonb(scan) ORDER BY scan.id) FROM attachment_scan_attempts scan WHERE scan.attachment_id=a.id))::text witness
 FROM attachments a ${socialDraftAttachmentJoin} LEFT JOIN upload_intents u ON u.id=a.upload_id AND u.company_id=a.company_id
 WHERE a.id=? AND d.company_id=? AND a.owner_type='social_draft'
 AND (d.published_post_id=? OR d.edit_post_id=? AND d.edit_saved_version IS NOT NULL OR d.comment_parent_post_id=? AND d.published_comment_id=?)
 AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp')`).bind(input.id,input.companyId,input.postId,input.postId,input.postId,input.commentId).first<{object_key:string;checksum_sha256:string;media_type:string;bytes:number;witness:string}>();
}
export function socialAgentImageSourceReadLocks(database:SqlDatabase){
 // Native audit/deferred effects may write these metadata sources. A writer
 // lease avoids cross-Company SHARE upgrades while private bytes are buffered.
 return database.prepare('LOCK TABLE attachments,upload_intents,attachment_scan_attempts IN SHARE ROW EXCLUSIVE MODE');
}
