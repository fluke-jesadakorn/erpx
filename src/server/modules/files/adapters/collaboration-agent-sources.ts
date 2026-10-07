import { socialDraftAttachmentJoin } from '../../collaboration/public/social-draft-file-projection';
import type { SqlDatabase } from '../../runtime/public/runtime/types';
export function collaborationAgentSources(db: SqlDatabase) {
    return { async legacyPostImages(post_id: string, post_id2: string, companyId: string) {
            return db.prepare(`SELECT a.id FROM attachments a ${socialDraftAttachmentJoin}
      WHERE (d.published_post_id=? OR (d.edit_post_id=? AND d.edit_saved_version IS NOT NULL)) AND d.company_id=?
        AND a.owner_type='social_draft' AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp') ORDER BY a.created_at,a.id`)
                .bind(post_id, post_id2, companyId).all<{
                id: string;
            }>();
        }, async imageSource(id: string, companyId: string, post_id: string, post_id2: string, post_id3: string, value: string | null) {
            return db.prepare(`SELECT a.object_key,a.checksum_sha256,a.media_type,a.bytes FROM attachments a ${socialDraftAttachmentJoin}
          WHERE a.id=? AND d.company_id=? AND a.owner_type='social_draft'
            AND (d.published_post_id=? OR (d.edit_post_id=? AND d.edit_saved_version IS NOT NULL) OR (d.comment_parent_post_id=? AND d.published_comment_id=?))
            AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp')`)
                .bind(id, companyId, post_id, post_id2, post_id3, value).first<{
                object_key: string;
                checksum_sha256: string;
                media_type: string;
                bytes: number;
            }>();
        } };
}
