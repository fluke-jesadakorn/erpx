import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { socialDraftFilesReadSql } from '@erpx/server-collaboration/public/social-draft-file-projection';
import { nativeWrite } from './native-files-command';
import { socialDraftImageExistsSql } from './social-draft-image-guard-sql';
import { SELECT_ATTACHMENT_ID_SQL } from './read-tail-r27-source-sql';
import { UPLOAD_INTENTS_EXISTS_HEAD_SQL } from './read-tail-r44-source-sql';
export function collaborationHomeSources(db: SqlDatabase) {
    return { revokeUnusedDraftFiles(now: number, recordId: string, keptAttachmentIds: string[]) {
            return nativeWrite('SF_REVOKE_DRAFT', [now, recordId, JSON.stringify(keptAttachmentIds)]);
        }, expireUnusedDraftUploads(now: number, recordId: string) {
            return nativeWrite('SF_EXPIRE_DRAFT_UPLOADS', [now, recordId]);
        }, async publishImages(ids: string[], id: string, companyId: string) {
            return db.prepare(`${SELECT_ATTACHMENT_ID_SQL} JOIN upload_intents u ON u.id=a.upload_id JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id
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
            return assertion(db, key, value, "NOT " + UPLOAD_INTENTS_EXISTS_HEAD_SQL + "owner_type='social_draft' AND owner_id=? AND status IN ('pending','uploaded') AND expires_at>?)", [id, now]);
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
            return db.prepare(`${SELECT_ATTACHMENT_ID_SQL} JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id
            WHERE d.published_post_id=? AND d.company_id=? AND a.owner_type='social_draft' AND a.revoked_at IS NULL`)
                .bind(id, companyId).all<{
                id: string;
            }>();
        }, async retainedImages(value: (string | null)[], id: string, id2: string, companyId: string) {
            return db.prepare(`${SELECT_ATTACHMENT_ID_SQL} JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id
        WHERE a.id IN (${value.map(() => "?").join(",")}) AND a.owner_type='social_draft'
          AND (d.published_post_id=? OR (d.edit_post_id=? AND d.edit_saved_version IS NOT NULL))
          AND d.company_id=? AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp')`)
                .bind(...value, id, id2, companyId).all<{
                id: string;
            }>();
        }, async uploadedImages(value: (string | null)[], editDraftId: string | undefined, userId: string, companyId: string, id: string, version: number) {
            return db.prepare(`${SELECT_ATTACHMENT_ID_SQL}
        JOIN upload_intents u ON u.id=a.upload_id JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id
        WHERE a.id IN (${value.map(() => "?").join(",")}) AND a.owner_type='social_draft' AND a.owner_id=?
          AND d.author_id=? AND d.company_id=? AND d.edit_post_id=? AND d.edit_post_version=? AND d.edit_saved_version IS NULL
          AND d.cancelled_at IS NULL AND a.revoked_at IS NULL AND a.media_type IN ('image/jpeg','image/png','image/webp')
          AND u.owner_type='social_draft' AND u.owner_id=d.id AND u.status='ready'`)
                .bind(...value, editDraftId, userId, companyId, id, version).all<{
                id: string;
            }>();
        }, guardRetainedImage(key: string, value: string, attachmentId: string | null, id: string, id2: string, companyId: string) {
            return assertion(db, key, value, `${socialDraftImageExistsSql}(${socialDraftFilesReadSql}) d ON d.id=a.owner_id WHERE a.id=? AND a.owner_type='social_draft' AND (d.published_post_id=? OR (d.edit_post_id=? AND d.edit_saved_version IS NOT NULL)) AND d.company_id=? AND a.revoked_at IS NULL)`, [attachmentId, id, id2, companyId]);
        }, guardUploadedImage(key: string, value: string, attachmentId: string | null, value2: string, userId: string, companyId: string, id: string, version: number) {
            return assertion(db, key, value, `${socialDraftImageExistsSql}upload_intents u ON u.id=a.upload_id JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id WHERE a.id=? AND a.owner_type='social_draft' AND a.owner_id=? AND d.author_id=? AND d.company_id=? AND d.edit_post_id=? AND d.edit_post_version=? AND d.edit_saved_version IS NULL AND d.cancelled_at IS NULL AND a.revoked_at IS NULL AND u.owner_type='social_draft' AND u.owner_id=d.id AND u.status='ready')`, [attachmentId, value2, userId, companyId, id, version]);
        }, revokeUnusedPostFiles(now: number, id: string, id2: string, value: (string | null)[]) {
            return nativeWrite('SF_REVOKE_POST', [now, id, id2, JSON.stringify(value.filter((entry): entry is string => typeof entry === 'string' && entry.length > 0))]);
        }, expireUnusedPostUploads(now: number, id: string, id2: string) {
            return nativeWrite('SF_EXPIRE_POST_UPLOADS', [now, id, id2]);
        }, async commentImages(value: (string | null)[], commentDraftId: string, companyId: string) {
            return db.prepare(`${SELECT_ATTACHMENT_ID_SQL} JOIN upload_intents u ON u.id=a.upload_id JOIN (${socialDraftFilesReadSql}) d ON d.id=a.owner_id WHERE a.id IN (${value.map(() => "?").join(",")}) AND a.owner_type='social_draft' AND a.owner_id=? AND d.company_id=? AND a.revoked_at IS NULL AND u.status='ready' AND a.media_type IN ('image/jpeg','image/png','image/webp')`)
                .bind(...value, commentDraftId, companyId).all<{
                id: string;
            }>();
        }, guardCommentImage(key: string, value: string, attachmentId: string | null, commentDraftId: string, companyId: string) { return assertion(db, key, value, `${socialDraftImageExistsSql}(${socialDraftFilesReadSql}) d ON d.id=a.owner_id WHERE a.id=? AND a.owner_type='social_draft' AND a.owner_id=? AND d.company_id=? AND a.revoked_at IS NULL)`, [attachmentId, commentDraftId, companyId]); }, revokeUnusedCommentFiles(now: number, commentDraftId: string, value: (string | null)[]) {
            return nativeWrite('SF_REVOKE_COMMENT', [now, commentDraftId, JSON.stringify(value.filter((entry): entry is string => typeof entry === 'string' && entry.length > 0))]);
        } };
}
