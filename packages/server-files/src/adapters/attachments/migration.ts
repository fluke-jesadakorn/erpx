import { nativeWrite, resolveNativeEffects } from '../native-files-command';
import { legacyAttachmentOwnerScope, legacyHoldingBrandCompanies } from '@erpx/server-files/adapters/attachments/migration-owner-sources';
import { attachmentMigrationcollaboration } from '@erpx/server-collaboration/public/attachment-migration-facts';
import { attachmentMigrationlegal } from '@erpx/server-legal/public/attachment-migration-facts';
import { attachmentMigrationpos } from '@erpx/server-pos/public/attachment-migration-facts';
import { attachmentMigrationpeoplepayroll } from '@erpx/server-people_payroll/public/attachment-migration-facts';
import "server-only";
import { createHash } from "node:crypto";
import sharp from "sharp";
import type { FileStore, SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { attachmentPolicy, attachmentRetentionSnapshot } from "@erpx/server-files/public/attachments/policy";
import { legacyAttachmentPolicy } from "@erpx/server-files/public/attachments/legacy-policy";
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { LegacySource } from '@erpx/server-files/domain/attachments/migration';
import { createAttachmentMigrationApplication, type AttachmentMigrationRepository } from '@erpx/server-files/application/attachments-migration';
function application(db: SqlDatabase) {
    const database = db, repository: AttachmentMigrationRepository<SqlStatement> = { readBackfillSpecializedAttachmentsMessaging() { return attachmentMigrationcollaboration(database).readBackfillSpecializedAttachmentsMessaging(); },
        readBackfillSpecializedAttachmentsDocuments() { return attachmentMigrationlegal(database).readBackfillSpecializedAttachmentsDocuments(); },
        readBackfillSpecializedAttachmentsExternal() { return attachmentMigrationlegal(database).readBackfillSpecializedAttachmentsExternal(); },
        readBackfillSpecializedAttachmentsImages() { return attachmentMigrationpos(database).readBackfillSpecializedAttachmentsImages(); },
        stageBackfillSpecializedAttachmentsEffect(setId: string, companyId: string, branchId: string | null, departmentId: string | null, targetType: "messaging_room" | "legal_item" | "product", targetId: string, componentKind: "record" | "action" | "line" | "event" | "message" | "publication", componentRef: string, purposeKey: string, targetVersion: number, version: number, classification: import("@erpx/server-files/domain/attachments/types").AttachmentClassification, value: string, key: string | null, version2: number | null, retainUntil: null, destructionMode: "review" | null, requiredCount: number, maxFiles: number, createdBy: string, createdAt: number, now: number) {
            return nativeWrite('MIG_001', [setId, companyId, branchId, departmentId, targetType, targetId, componentKind, componentRef, purposeKey, targetVersion, version, classification, value, key, version2, retainUntil, destructionMode, requiredCount, maxFiles, createdBy, createdAt, now]);
        },
        stageBackfillSpecializedAttachmentsEffect2(linkId: string, setId: string, createdBy: string, createdAt: number, expiresAt: number) { return nativeWrite('MIG_002', [linkId, setId, createdBy, createdAt, expiresAt]); },
        stageBackfillSpecializedAttachmentsEffect3(uploadId: string, targetType: "messaging_room" | "legal_item" | "product", targetId: string, purposeKey: string, originalName: string, mediaType: string, bytes: number, bytes2: number, checksum: string, objectKey: string, objectKey2: string, revision: number, createdBy: string, expiresAt: number, createdAt: number, now: number, targetVersion: number, setId: string, linkId: string, companyId: string, objectKey3: string) {
            return nativeWrite('MIG_003', [uploadId, targetType, targetId, purposeKey, originalName, mediaType, bytes, bytes2, checksum, objectKey, objectKey2, revision, createdBy, expiresAt, createdAt, now, targetVersion, setId, linkId, companyId, objectKey3]);
        },
        stageBackfillSpecializedAttachmentsEffect4(uploadId: string, linkId: string) { return nativeWrite('MIG_004', [uploadId, linkId]); },
        stageBackfillSpecializedAttachmentsEffect5(attachmentId: string, uploadId: string, targetType: "messaging_room" | "legal_item" | "product", targetId: string, purposeKey: string, objectKey: string, revision: number, originalName: string, mediaType: string, bytes: number, checksum: string, createdBy: string, value: 0 | 1, createdAt: number, companyId: string) {
            return nativeWrite('MIG_005', [attachmentId, uploadId, targetType, targetId, purposeKey, objectKey, revision, originalName, mediaType, bytes, checksum, createdBy, value, createdAt, companyId]);
        },
        stageBackfillSpecializedAttachmentsEffect6(attachmentId: string, linkId: string) { return nativeWrite('MIG_006', [attachmentId, linkId]); },
        stageBackfillSpecializedAttachmentsEffect7(scanId: string, attachmentId: string, now: number) { return nativeWrite('MIG_007', [scanId, attachmentId, now]); },
        stageBackfillSpecializedAttachmentsEffect8(value: string, companyId: string, attachmentId: string, objectKey: string, bytes: number, createdAt: number) { return nativeWrite('MIG_008', [value, companyId, attachmentId, objectKey, bytes, createdAt]); },
        stageBackfillSpecializedAttachmentsEffect9(value: string, companyId: string, attachmentId: string, linkId: string, setId: string, value2: string, now: number) { return nativeWrite('MIG_009', [value, companyId, attachmentId, linkId, setId, value2, now]); },
        stageBackfillSpecializedAttachmentsEffect10(setId: string, linkId: string, attachmentId: string, id: string, companyId: string) { return attachmentMigrationcollaboration(database).stageBackfillSpecializedAttachmentsEffect10(setId, linkId, attachmentId, id, companyId); },
        stageBackfillSpecializedAttachmentsEffect11(linkId: string, attachmentId: string, id: string, companyId: string) { return attachmentMigrationlegal(database).stageBackfillSpecializedAttachmentsEffect11(linkId, attachmentId, id, companyId); },
        stageBackfillSpecializedAttachmentsEffect12(linkId: string, attachmentId: string, id: string, companyId: string) { return attachmentMigrationlegal(database).stageBackfillSpecializedAttachmentsEffect12(linkId, attachmentId, id, companyId); },
        stageBackfillSpecializedAttachmentsEffect13(setId: string, linkId: string, attachmentId: string, checksum: string, id: string) { return attachmentMigrationpos(database).stageBackfillSpecializedAttachmentsEffect13(setId, linkId, attachmentId, checksum, id) as unknown as SqlStatement; },
        stageBackfillSpecializedAttachmentsEffect14(value: string, attachmentId: string, companyId: string, objectKey: string, checksum: string, checksum2: string, bytes: number, createdBy: string, createdAt: number) {
            return nativeWrite('MIG_023', [value, attachmentId, companyId, objectKey, checksum, checksum2, bytes, createdBy, createdAt]);
        },
        stageBackfillSpecializedAttachmentsEffect15(value: string, companyId: string, attachmentId: string, objectKey: string, bytes: number, createdAt: number) {
            return nativeWrite('MIG_024', [value, companyId, attachmentId, objectKey, bytes, createdAt]);
        },
        stageBackfillSpecializedAttachmentsEffect16(boundAt: number | null, value: string, id: string, linkId: string) { return nativeWrite('MIG_010', [boundAt, value, id, linkId]); },
        stageBackfillSpecializedAttachmentsEffect17(boundAt: number | null, value: string, now: number, setId: string) { return nativeWrite('MIG_011', [boundAt, value, now, setId]); },
        readBackfillLegacyAttachmentsSources() { return database.prepare("SELECT id,upload_id,owner_type,owner_id,purpose,object_key,original_name,media_type,bytes,checksum_sha256,created_by,created_at FROM attachments WHERE company_id IS NULL AND legacy_sanitized_only=false ORDER BY created_at,id").all<LegacySource>(); },
        stageBackfillLegacyAttachmentsEffect(id: string) { return nativeWrite('MIG_012', [id]).run(); },
        stageBackfillLegacyAttachmentsEffect2(id: string) { return nativeWrite('MIG_013', [id]).run(); },
        stageBackfillLegacyAttachmentsEffect3(id: string) { return nativeWrite('MIG_014', [id]).run(); },
        stageBackfillLegacyAttachmentsEffect4(id: string) { return nativeWrite('MIG_015', [id]).run(); },
        stageBackfillLegacyAttachmentsEffect5(setId: string, companyId: string, branchId: string | null, departmentId: string | null, owner_type: string, owner_id: string, componentKind: "record" | "action" | "line" | "event" | "message" | "publication", componentRef: string | null, purposeKey: string, version: number, version2: number, classification: import("@erpx/server-files/domain/attachments/types").AttachmentClassification, value: string, key: string | null, version3: number | null, retainUntil: null, destructionMode: "review" | null, requiredCount: number, maxFiles: number, created_by: string, created_at: number, now: number, now2: number, value2: string) {
            return nativeWrite('MIG_016', [setId, companyId, branchId, departmentId, owner_type, owner_id, componentKind, componentRef, purposeKey, version, version2, classification, value, key, version3, retainUntil, destructionMode, requiredCount, maxFiles, created_by, created_at, now, now2, value2]);
        },
        stageBackfillLegacyAttachmentsEffect6(linkId: string, setId: string, id: string, upload_id: string, created_by: string, created_at: number, value: number, now: number, owner_id: string) {
            return nativeWrite('MIG_017', [linkId, setId, id, upload_id, created_by, created_at, value, now, owner_id]);
        },
        stageBackfillLegacyAttachmentsEffect7(companyId: string, id: string) { return nativeWrite('MIG_018', [companyId, id]); },
        stageBackfillLegacyAttachmentsEffect8(setId: string, linkId: string, companyId: string, object_key: string, now: number, upload_id: string) { return nativeWrite('MIG_019', [setId, linkId, companyId, object_key, now, upload_id]); },
        stageBackfillLegacyAttachmentsEffect9(value: string, id: string, now: number) { return nativeWrite('MIG_020', [value, id, now]); },
        stageBackfillLegacyAttachmentsEffect10(value: string, companyId: string, id: string, object_key: string, bytes: number, created_at: number) { return nativeWrite('MIG_021', [value, companyId, id, object_key, bytes, created_at]); },
        stageBackfillLegacyAttachmentsEffect11(value: string, companyId: string, id: string, linkId: string, setId: string, value2: string, now: number) { return nativeWrite('MIG_022', [value, companyId, id, linkId, setId, value2, now]); },
        readReconcileAttachmentStorageRows() { return database.prepare("SELECT id,object_key,preview_object_key,preview_bytes,preview_checksum_sha256,bytes,checksum_sha256,source_state,company_id,legacy_sanitized_only,owner_type,owner_id,purpose,media_type FROM attachments ORDER BY id").all<{
            id: string;
            object_key: string;
            preview_object_key: string | null;
            preview_bytes: number | null;
            preview_checksum_sha256: string | null;
            bytes: number;
            checksum_sha256: string;
            source_state: string;
            company_id: string | null;
            legacy_sanitized_only: boolean;
            owner_type: string;
            owner_id: string;
            purpose: string;
            media_type: string;
        }>(); },
        readReconcileAttachmentStorageScanRows() { return database.prepare("SELECT DISTINCT ON (attachment_id) attachment_id,attempt,state,result_code,error_code FROM attachment_scan_attempts ORDER BY attachment_id,attempt DESC").all<{
            attachment_id: string;
            attempt: number;
            state: string;
            result_code: string | null;
            error_code: string | null;
        }>(); },
        readReconcileAttachmentStorageEffect() {
            return database.prepare(`SELECT link.id FROM attachment_links link JOIN attachment_sets set_row ON set_row.id=link.set_id
    LEFT JOIN attachments source ON source.id=link.attachment_id
    LEFT JOIN upload_intents intent ON intent.id=link.upload_intent_id
    WHERE (link.attachment_id IS NOT NULL AND (source.id IS NULL OR source.company_id IS DISTINCT FROM set_row.company_id))
       OR (link.upload_intent_id IS NOT NULL AND (intent.id IS NULL OR intent.company_id IS DISTINCT FROM set_row.company_id))
    ORDER BY link.id`).all<{
                id: string;
            }>();
        },
        readReconcileAttachmentStorageEffect2() {
            return database.prepare(`SELECT intent.id FROM upload_intents intent
    LEFT JOIN attachment_sets set_row ON set_row.id=intent.attachment_set_id
    LEFT JOIN attachment_links link ON link.id=intent.attachment_link_id
    WHERE intent.attachment_set_id IS NOT NULL AND (
      set_row.id IS NULL OR intent.company_id IS DISTINCT FROM set_row.company_id
      OR intent.attachment_link_id IS NULL OR link.id IS NULL OR link.set_id IS DISTINCT FROM set_row.id)
    ORDER BY intent.id`).all<{
                id: string;
            }>();
        },
        readReconcileAttachmentStorageEffect3() {
            return database.prepare(`SELECT ledger.id FROM attachment_storage_ledger ledger
    LEFT JOIN attachments source ON source.id=ledger.attachment_id
    WHERE source.id IS NULL OR (source.company_id IS NOT NULL AND ledger.company_id IS DISTINCT FROM source.company_id) ORDER BY ledger.id`).all<{
                id: string;
            }>();
        },
        readReconcileAttachmentStorageEffect4() {
            return database.prepare(`SELECT derivative.id FROM attachment_derivatives derivative
    LEFT JOIN attachments source ON source.id=derivative.attachment_id
    WHERE source.id IS NULL OR (source.company_id IS NOT NULL AND derivative.company_id IS DISTINCT FROM source.company_id) ORDER BY derivative.id`).all<{
                id: string;
            }>();
        },
        readReconcileAttachmentStorageEffect5() {
            return database.prepare(`SELECT publication.id FROM attachment_publications publication
    LEFT JOIN attachment_links link ON link.id=publication.source_link_id
    LEFT JOIN attachment_sets set_row ON set_row.id=link.set_id
    WHERE set_row.id IS NULL OR publication.company_id IS DISTINCT FROM set_row.company_id ORDER BY publication.id`).all<{
                id: string;
            }>();
        },
        readReconcileAttachmentStorageStoredLedgerMatches(companyId: string | null, attachmentId: string | null, variant: string, objectKey: string, bytes: number) {
            return database.prepare("SELECT 1 FROM attachment_storage_ledger WHERE company_id=? AND attachment_id=? AND variant=? AND object_key=? AND bytes=? AND state='stored'").bind(companyId, attachmentId, variant, objectKey, bytes).first();
        },
        readReconcileAttachmentStorageEffect6() { return database.prepare("SELECT to_regclass('attachment_orphan_preservations') present").first("present"); },
        readReconcileAttachmentStoragePreserved() {
            return database.prepare(`SELECT p.* FROM attachment_orphan_preservations p
      WHERE EXISTS(SELECT 1 FROM attachment_orphan_preservation_events e WHERE e.preservation_id=p.id AND e.event_kind='preserved')
        AND NOT EXISTS(SELECT 1 FROM attachment_orphan_preservation_events e WHERE e.preservation_id=p.id AND e.event_kind='restored')`).all<{
                id: string;
                object_key: string;
                object_sha256: string;
                manifest_sha256: string;
                bytes: number;
                preserved_directory: string;
                preservation_mode: string;
            }>();
        },
        readReconcileAttachmentStoragePublications() {
            return database.prepare(`SELECT publication.id,publication.company_id,publication.derivative_object_key,publication.derivative_bytes,publication.derivative_checksum_sha256,link.attachment_id
    FROM attachment_publications publication LEFT JOIN attachment_links link ON link.id=publication.source_link_id`).all<{
                id: string;
                company_id: string;
                derivative_object_key: string;
                derivative_bytes: number;
                derivative_checksum_sha256: string;
                attachment_id: string | null;
            }>();
        },
        readReconcileAttachmentStorageDerivatives() {
            return database.prepare(`SELECT derivative.id,derivative.attachment_id,derivative.company_id,derivative.object_key,derivative.bytes,derivative.checksum_sha256
    FROM attachment_derivatives derivative JOIN attachments source ON source.id=derivative.attachment_id
    WHERE source.source_state<>'destroyed' ORDER BY derivative.id`).all<{
                id: string;
                attachment_id: string;
                company_id: string;
                object_key: string;
                bytes: number;
                checksum_sha256: string;
            }>();
        },
        readReconcileAttachmentStoragePaymentExports() { return attachmentMigrationpeoplepayroll(database).readReconcileAttachmentStoragePaymentExports(); },
        readReconcileAttachmentStoragePayrollReviewExports() { return attachmentMigrationpeoplepayroll(database).readReconcileAttachmentStoragePayrollReviewExports(); },
        readReconcileAttachmentStorageIntents() { return database.prepare("SELECT staging_key,quarantine_key,final_key FROM upload_intents WHERE status IN ('pending','uploaded')").all<{
            staging_key: string;
            quarantine_key: string | null;
            final_key: string | null;
        }>(); },
        readReconcileAttachmentStorageEffect7() { return database.prepare("SELECT COALESCE(sum(bytes),0) FROM attachment_storage_ledger WHERE state='stored'").first("coalesce"); },
        readReconcileAttachmentStorageEffect8() { return database.prepare("SELECT count(*) FROM (SELECT object_key FROM attachment_storage_ledger WHERE state='stored' GROUP BY object_key HAVING count(*)>1) duplicates").first("count"); } };
    return createAttachmentMigrationApplication<SqlStatement>({ repository, clock: { now: Date.now }, policy: attachmentPolicy, legacyPolicy: legacyAttachmentPolicy, retention: attachmentRetentionSnapshot, ownerScope: source => legacyAttachmentOwnerScope(db, source), brandCompanies: () => legacyHoldingBrandCompanies(db), digest: value => createHash('sha256').update(value).digest('hex'), bytesDigest: buffer => sha256(new Uint8Array(buffer).slice().buffer), thumbnailChecksum: async (bytes) => createHash('sha256').update(await sharp(bytes, { limitInputPixels: 25000000, animated: false }).rotate().resize(480, 480, { fit: 'cover' }).webp({ quality: 82 }).toBuffer()).digest('hex'), preservationMetadataDigest: head => requireOrphan().preservationMetadataDigest(head), verifiedPreservedDirectory: (...args) => requireOrphan().verifiedPreservedDirectory(...args), batch: async effects => { const resolved = await resolveNativeEffects(effects, { companyId: '', key: 'files-migration', now: Date.now() }); return db.batch(resolved.statements); } });
}
import * as orphan from '@erpx/server-files/public/attachments/orphan-preservation';
const requireOrphan = () => orphan;
export function backfillSpecializedAttachments(db: SqlDatabase, options: {
    apply?: boolean;
    files?: FileStore;
} = {}) { return application(db).backfillSpecializedAttachments(options); }
export function backfillLegacyAttachments(db: SqlDatabase, options: {
    apply?: boolean;
} = {}) { return application(db).backfillLegacyAttachments(options); }
export function reconcileAttachmentStorage(db: SqlDatabase, files: FileStore, options: {
    verifyChecksums?: boolean;
} = {}) { return application(db).reconcileAttachmentStorage(files, options); }
export type { AttachmentUnavailableSource } from '@erpx/server-files/domain/attachments/migration';
