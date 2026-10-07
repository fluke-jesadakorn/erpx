import type { SqlDatabase } from '../../runtime/public/runtime/types';
import { assertion } from '../../workflow/public/commands';
import { socialDraftFilesReadSql } from '../../collaboration/public/social-draft-file-projection';
export function collaborationHomeSources(db: SqlDatabase) {
    return { revokeCommentFiles(now: number, id: string) { return db.prepare("UPDATE attachments SET revoked_at=? WHERE owner_type='social_draft' AND owner_id=? AND revoked_at IS NULL").bind(now, id); }, expireCommentUploads(now: number, id: string) { return db.prepare("UPDATE upload_intents SET status='expired',updated_at=? WHERE owner_type='social_draft' AND owner_id=? AND status='ready'").bind(now, id); }, revokeEditFiles(now: number, id: string) { return db.prepare("UPDATE attachments SET revoked_at=? WHERE owner_type='social_draft' AND owner_id=? AND revoked_at IS NULL").bind(now, id); }, expireEditUploads(now: number, id: string) { return db.prepare("UPDATE upload_intents SET status='expired',updated_at=? WHERE owner_type='social_draft' AND owner_id=? AND status='ready'").bind(now, id); }, revokeUnusedDraftFiles(now: number, recordId: string, keptAttachmentIds: string[]) {
            return db.prepare(`UPDATE attachments SET revoked_at=? WHERE owner_type='social_draft' AND owner_id=? AND revoked_at IS NULL${keptAttachmentIds.length ? ` AND id NOT IN (${keptAttachmentIds.map(() => "?").join(",")})` : ""}`)
                .bind(now, recordId, ...keptAttachmentIds);
        }, expireUnusedDraftUploads(now: number, recordId: string) {
            return db.prepare("UPDATE upload_intents u SET status='expired',updated_at=? WHERE u.owner_type='social_draft' AND u.owner_id=? AND u.status='ready' AND NOT EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=u.id AND a.revoked_at IS NULL)")
                .bind(now, recordId);
        }, async publishImages(ids: string[], id: string, companyId: string) {
            return db.prepare(`SELECT a.id FROM attachments a JOIN upload_intents u ON u.id=a.upload_id JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id
      WHERE a.id IN (${ids.map(() => "?").join(",")}) AND a.owner_type='social_draft' AND a.owner_id=? AND d.company_id=? AND a.revoked_at IS NULL
      AND a.media_type IN ('image/jpeg','image/png','image/webp') AND u.owner_type='social_draft' AND u.owner_id=d.id AND u.status='ready'`)
                .bind(...ids, id, companyId).all<{
                id: string;
            }>();
        }, guardPublishImage(key: string, value: string, attachmentId: string | null, id: string, companyId: string) {
            return assertion(db, key, value, `EXISTS(SELECT 1 FROM attachments a JOIN upload_intents u ON u.id=a.upload_id JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id
          WHERE a.id=? AND a.owner_type='social_draft' AND a.owner_id=? AND d.company_id=? AND a.revoked_at IS NULL
          AND a.media_type IN ('image/jpeg','image/png','image/webp') AND u.owner_type='social_draft' AND u.owner_id=d.id AND u.status='ready')`, [attachmentId, id, companyId]);
        }, guardNoPendingUploads(key: string, value: "uploads", id: string, now: number) {
            return assertion(db, key, value, "NOT EXISTS(SELECT 1 FROM upload_intents WHERE owner_type='social_draft' AND owner_id=? AND status IN ('pending','uploaded') AND expires_at>?)", [id, now]);
        }, async images(allowed: string[], allowed2: string[]) {
            return db.prepare(`SELECT COALESCE(d.published_post_id,d.edit_post_id) AS post_id,a.id,a.original_name,a.media_type FROM attachments a JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id
      WHERE a.owner_type='social_draft' AND (d.published_post_id IN (${allowed.map(() => '?').join(',')}) OR (d.edit_post_id IN (${allowed.map(() => '?').join(',')}) AND d.edit_saved_version IS NOT NULL))
        AND a.revoked_at IS NULL ORDER BY a.created_at,a.id`).bind(...allowed, ...allowed2)
                .all<{
                post_id: string;
                id: string;
                original_name: string;
                media_type: string;
            }>();
        }, async legacyImages(id: string, companyId: string) {
            return db.prepare(`SELECT a.id FROM attachments a JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id
            WHERE d.published_post_id=? AND d.company_id=? AND a.owner_type='social_draft' AND a.revoked_at IS NULL`)
                .bind(id, companyId).all<{
                id: string;
            }>();
        }, async retainedImages(value: (string | null)[], id: string, id2: string, companyId: string) {
            return db.prepare(`SELECT a.id FROM attachments a JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id
        WHERE a.id IN (${value.map(() => "?").join(",")}) AND a.owner_type='social_draft'
          AND (d.published_post_id=? OR (d.edit_post_id=? AND d.edit_saved_version IS NOT NULL))
          AND d.company_id=? AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp')`)
                .bind(...value, id, id2, companyId).all<{
                id: string;
            }>();
        }, async uploadedImages(value: (string | null)[], editDraftId: string | undefined, userId: string, companyId: string, id: string, version: number) {
            return db.prepare(`SELECT a.id FROM attachments a
        JOIN upload_intents u ON u.id=a.upload_id JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id
        WHERE a.id IN (${value.map(() => "?").join(",")}) AND a.owner_type='social_draft' AND a.owner_id=?
          AND d.author_id=? AND d.company_id=? AND d.edit_post_id=? AND d.edit_post_version=? AND d.edit_saved_version IS NULL
          AND d.cancelled_at IS NULL AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp')
          AND u.owner_type='social_draft' AND u.owner_id=d.id AND u.status='ready'`)
                .bind(...value, editDraftId, userId, companyId, id, version).all<{
                id: string;
            }>();
        }, guardRetainedImage(key: string, value: string, attachmentId: string | null, id: string, id2: string, companyId: string) {
            return assertion(db, key, value, `EXISTS(SELECT 1 FROM attachments a JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id WHERE a.id=? AND a.owner_type='social_draft' AND (d.published_post_id=? OR (d.edit_post_id=? AND d.edit_saved_version IS NOT NULL)) AND d.company_id=? AND a.revoked_at IS NULL)`, [attachmentId, id, id2, companyId]);
        }, guardUploadedImage(key: string, value: string, attachmentId: string | null, value2: string, userId: string, companyId: string, id: string, version: number) {
            return assertion(db, key, value, `EXISTS(SELECT 1 FROM attachments a JOIN upload_intents u ON u.id=a.upload_id JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id WHERE a.id=? AND a.owner_type='social_draft' AND a.owner_id=? AND d.author_id=? AND d.company_id=? AND d.edit_post_id=? AND d.edit_post_version=? AND d.edit_saved_version IS NULL AND d.cancelled_at IS NULL AND a.revoked_at IS NULL AND u.owner_type='social_draft' AND u.owner_id=d.id AND u.status='ready')`, [attachmentId, value2, userId, companyId, id, version]);
        }, revokeUnusedPostFiles(now: number, id: string, id2: string, value: (string | null)[]) {
            return db.prepare(`UPDATE attachments SET revoked_at=? WHERE owner_type='social_draft' AND revoked_at IS NULL
          AND owner_id IN (SELECT id FROM (${socialDraftFilesReadSql}) drafts WHERE published_post_id=? OR edit_post_id=?)
          ${value.length ? `AND id NOT IN (${value.map(() => "?").join(",")})` : ""}`)
                .bind(now, id, id2, ...value);
        }, expireUnusedPostUploads(now: number, id: string, id2: string) {
            return db.prepare(`UPDATE upload_intents u SET status='expired',updated_at=? WHERE u.owner_type='social_draft' AND u.status='ready'
          AND u.owner_id IN (SELECT id FROM (${socialDraftFilesReadSql}) drafts WHERE published_post_id=? OR edit_post_id=?)
          AND NOT EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=u.id AND a.revoked_at IS NULL)`)
                .bind(now, id, id2);
        }, async commentImages(value: (string | null)[], commentDraftId: string, companyId: string) {
            return db.prepare(`SELECT a.id FROM attachments a JOIN upload_intents u ON u.id=a.upload_id JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id WHERE a.id IN (${value.map(() => "?").join(",")}) AND a.owner_type='social_draft' AND a.owner_id=? AND d.company_id=? AND a.revoked_at IS NULL AND u.status='ready' AND a.media_type IN ('image/jpeg','image/png','image/webp')`)
                .bind(...value, commentDraftId, companyId).all<{
                id: string;
            }>();
        }, guardCommentImage(key: string, value: string, attachmentId: string | null, commentDraftId: string, companyId: string) { return assertion(db, key, value, `EXISTS(SELECT 1 FROM attachments a JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id WHERE a.id=? AND a.owner_type='social_draft' AND a.owner_id=? AND d.company_id=? AND a.revoked_at IS NULL)`, [attachmentId, commentDraftId, companyId]); }, revokeUnusedCommentFiles(now: number, commentDraftId: string, value: (string | null)[]) {
            return db.prepare(`UPDATE attachments SET revoked_at=? WHERE owner_type='social_draft' AND owner_id=? AND revoked_at IS NULL${value.length ? ` AND id NOT IN (${value.map(() => "?").join(",")})` : ""}`)
                .bind(now, commentDraftId, ...value);
        } };
}
