import { nativeWrite } from '../native-files-command';
import { hospitalityOwnerProjectionSources as legalProjectionSources } from "@erpx/server-legal/public/hospitality-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { AttachmentSourceState, AttachmentComponentKind } from "@erpx/server-files/domain/attachments/types";
import type { SetRow, LinkRow } from "@erpx/server-files/domain/attachments/service-models";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {LINK_ATTACHMENT_SELECT_SQL, LINK_SET_ATTACHMENT_EXISTS_SQL, LINK_SET_ID_SELECT_SQL, LINK_SET_WHERE_EXISTS_SQL} from "../read-tail-dedupe-sql";
import {attachmentsByIdCompanySql} from "../read-tail-r26-source-sql";
import {SELECT_ATTACHMENT_ID_SQL} from "../read-tail-r27-source-sql";
import {LINK_SET_JOIN_SQL} from "../read-tail-r29-source-sql";
import {attachmentPublicationByIdSql} from "../read-tail-r31-source-sql";
import {ATTACHMENT_LINK_PROJECTION_HEAD_SQL} from "../read-tail-r37-source-sql";
import {ATTACHMENT_EVENT_EVENT_HEAD_SQL} from "../read-tail-r38-source-sql";
import {SANITIZATION_ATTESTED_HEAD_SQL} from "../read-tail-r40-source-sql";
import {LEDGER_OBJECT_KEY_PROJECTION_HEAD_SQL} from "../read-tail-r42-source-sql";
import {ATTACHMENT_STAR_ID_HEAD_SQL} from "../read-tail-r44-source-sql";
import {ATTACHMENT_EVENTS_WHERE_HEAD_SQL,ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL} from "../read-tail-r45-source-sql";
import {SET_DRAFT_BOUND_COUNT_SQL} from "../read-tail-r46-source-sql";
import {SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL} from "../read-tail-r47-source-sql";
const setByIdSql = ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL + "id=? AND company_id=?";
export const usedStorageSql = "SELECT COALESCE(sum(bytes),0) FROM attachment_storage_ledger WHERE company_id=? AND state IN ('reserved','stored')";
export const pendingStorageSql = "SELECT COALESCE(sum(declared_bytes),0) FROM upload_intents WHERE company_id=? AND status='pending' AND expires_at>?";
const advisoryLockSql = "SELECT pg_advisory_xact_lock(hashtextextended(?,0))";
const attachmentChecksumSql = attachmentsByIdCompanySql('checksum_sha256,source_state');
const setLockSql = "SELECT state FROM attachment_sets WHERE id=? AND company_id=? FOR UPDATE";
const publicationByIdSql = attachmentPublicationByIdSql(true);
const publicationOrderVersionSql = "SELECT version FROM attachment_publication_orders WHERE company_id=? AND target_type=? AND target_id=?";
const uploadIntentForSetSql = "SELECT * FROM upload_intents WHERE id=? AND attachment_set_id IS NOT NULL AND created_by=? AND company_id=?";
const attachmentLinkSql = `${ATTACHMENT_LINK_PROJECTION_HEAD_SQL}s.state set_state,s.company_id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=? AND s.company_id=?`;
/** Byte-identical latest-change witness shared by the attested sanitization
 * read and its command assertion. The rendered SQL and binds are unchanged;
 * each site still prepares its own statement. */
const changedCreatedAtSql = "(SELECT max(changed.created_at) FROM attachment_events changed WHERE changed.set_id=attested.set_id AND changed.action IN ('file.reserved','link.reused','link.withdrawn'))";
/** Parameterized set.created compatibility witness shared by the candidate
 * read and its count assertion; only the trailing predicate differs. Each site
 * still prepares its own statement with its own binds. */
const setCreatedCompatibilitySql = (value: string) =>
  `EXISTS(${ATTACHMENT_EVENT_EVENT_HEAD_SQL}set_id=candidate.id AND event.company_id=candidate.company_id AND event.action='set.created' AND COALESCE((event.details_json::jsonb->>'compatibilityAdapter')::boolean,false)=${value})`;
const sanitizationAttestedSql = `${SANITIZATION_ATTESTED_HEAD_SQL}
        AND attested.created_at>=COALESCE(${changedCreatedAtSql},0)
        ORDER BY attested.created_at DESC LIMIT 1`;
export function repository(db: SqlDatabase) {
    return {
        readAttachmentSetAlias: (facts:{id:string;companyId:string;userId:string}) => db.prepare(`SELECT source.* FROM attachment_events receipt JOIN attachment_sets source ON source.id=receipt.set_id AND source.company_id=receipt.company_id AND source.created_by=receipt.actor_id
          WHERE receipt.id=? AND receipt.company_id=? AND receipt.actor_id=? AND receipt.action='set.reused' AND receipt.details_json::jsonb->>'requestedSetId'=?`).bind(`set-alias-${facts.id}`,facts.companyId,facts.userId,facts.id).first<SetRow>(),
        stepAttachmentSetAlias: (facts:{id:string;companyId:string;userId:string;setId:string;now:number}) => nativeWrite('SR_001', [`set-alias-${facts.id}`,facts.userId,facts.id,facts.now,facts.setId,facts.companyId,facts.userId]),
        readAttachmentSets1: (facts: {
            id: string;
            companyId: string;
        }) => { return db.prepare(setByIdSql).bind(facts.id, facts.companyId).first<SetRow>(); },
        readAttachmentLinks5: (facts: {
            id: string;
            companyId: string;
        }) => {
            return db.prepare(`SELECT l.*,a.original_name,a.media_type,a.bytes,a.source_state,a.preview_object_key,
      u.original_name AS intent_original_name,u.media_type AS intent_media_type,u.declared_bytes
    FROM attachment_links l
    JOIN attachment_sets s ON s.id=l.set_id
    LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
    LEFT JOIN upload_intents u ON u.id=l.upload_intent_id AND u.company_id=s.company_id
    WHERE l.set_id=? AND s.company_id=?
      AND (l.attachment_id IS NULL OR a.id IS NOT NULL)
      AND (l.upload_intent_id IS NULL OR u.id IS NOT NULL)
    ORDER BY l.created_at,l.id`).bind(facts.id, facts.companyId).all<LinkRow & {
                intent_original_name: string | null;
                intent_media_type: string | null;
            }>();
        },
        readAttachmentEvents6: (facts: {
            id: string;
            company_id: string;
        }) => {
            return db.prepare(sanitizationAttestedSql).bind(facts.id, facts.company_id).first();
        },
        readAttachmentSets7: (facts: {
            actor: WorkContext;
            input: {
                targetType: string;
                targetId: string;
                purposeKey?: string;
                componentKind?: AttachmentComponentKind;
                expectedTargetVersion?: number;
            };
            readOnly: boolean;
            leaveReview: boolean;
        }) => {
            const { actor, input, readOnly, leaveReview } = facts;
            const filters = ["company_id=?", "target_type=?", "target_id=?", readOnly ? "state='bound'" : leaveReview ? "state IN ('draft','bound')" : "(state='bound' OR (state='draft' AND created_by=?))"], values: unknown[] = [actor.companyId, input.targetType, input.targetId];
            if (!readOnly && !leaveReview)
                values.push(actor.userId);
            if (input.purposeKey) {
                filters.push("purpose_key=?");
                values.push(input.purposeKey);
            }
            if (input.componentKind) {
                filters.push("component_kind=?");
                values.push(input.componentKind);
            }
            if (input.expectedTargetVersion) {
                filters.push("expected_target_version=?");
                values.push(input.expectedTargetVersion);
            }
            return db.prepare(`${ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL}${filters.join(" AND ")} ORDER BY created_at DESC,id DESC`).bind(...values).all<SetRow>();
        },
        lock8: (facts: {
            arg1: string;
        }) => { return db.prepare(advisoryLockSql).bind(facts.arg1).run(); },
        readAttachmentSets9: (facts: {
            id: string;
            companyId: string;
        }) => { return db.prepare(setByIdSql).bind(facts.id, facts.companyId).first<SetRow>(); },
        readAttachmentEvents10: (facts: {
            id: string;
            companyId: string;
        }) => { return db.prepare(ATTACHMENT_EVENTS_WHERE_HEAD_SQL + "set_id=? AND company_id=? AND action='set.created' AND COALESCE((details_json::jsonb->>'compatibilityAdapter')::boolean,false)=true LIMIT 1").bind(facts.id, facts.companyId).first(); },
        readAttachmentSets11: (facts: {
            companyId: string;
            targetType: string;
            targetId: string;
            componentKind: "record" | "action" | "line" | "event" | "message" | "publication";
            componentRef: string | null;
            purposeKey: string;
            expectedTargetVersion: number;
            userId: string;
            arg9: boolean;
        }) => {
            return db.prepare(`SELECT candidate.* FROM attachment_sets candidate WHERE candidate.company_id=? AND candidate.target_type=? AND candidate.target_id=? AND candidate.component_kind=?
      AND COALESCE(candidate.component_ref,'')=COALESCE(?,'') AND candidate.purpose_key=? AND candidate.expected_target_version=? AND candidate.created_by=? AND candidate.state='draft'
      AND ${setCreatedCompatibilitySql('?')}`).bind(facts.companyId, facts.targetType, facts.targetId, facts.componentKind, facts.componentRef, facts.purposeKey, facts.expectedTargetVersion, facts.userId, facts.arg9).first<SetRow>();
        },
        stepAttachmentSets12: (facts: {
            id: string;
            companyId: string;
            branchId: string | null;
            departmentId: string | null;
            targetType: string;
            targetId: string;
            componentKind: "record" | "action" | "line" | "event" | "message" | "publication";
            componentRef: string | null;
            purposeKey: string;
            expectedTargetVersion: number;
            version: number;
            classification: import("@erpx/server-files/domain/attachments/types").AttachmentClassification;
            arg13: string;
            key: string | null;
            versionValue15: number | null;
            retainUntil: null;
            destructionMode: "review" | null;
            requiredCount: number;
            maxFiles: number;
            userId: string;
            now: number;
        }) => {
            return nativeWrite('SR_002', [facts.id, facts.companyId, facts.branchId, facts.departmentId, facts.targetType, facts.targetId, facts.componentKind, facts.componentRef, facts.purposeKey, facts.expectedTargetVersion, facts.version, facts.classification, facts.arg13, facts.key, facts.versionValue15, facts.retainUntil, facts.destructionMode, facts.requiredCount, facts.maxFiles, facts.userId, facts.now, facts.now]);
        },
        stepAttachmentEvents13: (facts: {
            arg1: string;
            companyId: string;
            id: string;
            userId: string;
            arg5: "set.created";
            arg6: string;
            now: number;
        }) => { return nativeWrite('SR_003', [facts.arg1, facts.companyId, facts.id, facts.userId, facts.arg5, facts.arg6, facts.now]); },
        readAttachmentLinks14: (facts: {
            supersedesLinkId: string;
            companyId: string;
            target_type: string;
            target_id: string;
            purpose_key: string;
        }) => { return db.prepare(`${LINK_SET_ID_SELECT_SQL} WHERE l.id=? AND l.state='bound' AND s.company_id=? AND s.target_type=? AND s.target_id=? AND s.purpose_key=?`).bind(facts.supersedesLinkId, facts.companyId, facts.target_type, facts.target_id, facts.purpose_key).first(); },
        lock15: (facts: {
            arg1: string;
        }) => { return db.prepare(advisoryLockSql).bind(facts.arg1).run(); },
        readUploadIntents16: (facts: {
            token: string;
            companyId: string;
            userId: string;
        }) => {
            return db.prepare(`SELECT u.attachment_set_id,u.attachment_link_id,u.original_name,u.media_type,u.declared_bytes,u.expires_at,
      l.caption,l.supersedes_link_id FROM upload_intents u JOIN attachment_links l ON l.id=u.attachment_link_id
      WHERE u.id=? AND u.company_id=? AND u.created_by=?`).bind(facts.token, facts.companyId, facts.userId).first<{
                attachment_set_id: string;
                attachment_link_id: string;
                original_name: string;
                media_type: string;
                declared_bytes: number;
                expires_at: number;
                caption: string | null;
                supersedes_link_id: string | null;
            }>();
        },
        readAttachmentLinks17: (facts: {
            setId: string;
            companyId: string;
        }) => { return db.prepare(SET_DRAFT_BOUND_COUNT_SQL(true)).bind(facts.setId, facts.companyId).first("count"); },
        readAttachmentStorageLedger18: (facts: {
            companyId: string;
        }) => { return db.prepare(usedStorageSql).bind(facts.companyId).first("coalesce"); },
        readUploadIntents19: (facts: {
            companyId: string;
            now: number;
        }) => { return db.prepare(pendingStorageSql).bind(facts.companyId, facts.now).first("coalesce"); },
        stepAttachmentLinks21: (facts: {
            linkId: string;
            setId: string;
            caption: string | null;
            arg4: string | null;
            userId: string;
            now: number;
            draftExpires: number;
        }) => { return nativeWrite('SR_004', [facts.linkId, facts.setId, facts.caption, facts.arg4, facts.userId, facts.now, facts.draftExpires]); },
        stepUploadIntents22: (facts: {
            token: string;
            target_type: string;
            target_id: string;
            purpose_key: string;
            filename: string;
            mediaType: string;
            bytes: number;
            objectKey: string;
            arg9: "pending";
            userId: string;
            reserveExpires: number;
            now: number;
            expected_target_version: number;
            setId: string;
            linkId: string;
            companyId: string;
        }) => {
            return nativeWrite('SR_005', [facts.token, facts.target_type, facts.target_id, facts.purpose_key, facts.filename, facts.mediaType, facts.bytes, facts.objectKey, facts.arg9, facts.userId, facts.reserveExpires, facts.now, facts.now, facts.expected_target_version, facts.setId, facts.linkId, facts.companyId, facts.objectKey]);
        },
        stepAttachmentLinks23: (facts: {
            token: string;
            linkId: string;
        }) => { return nativeWrite('SR_006', [facts.token, facts.linkId]); },
        stepAttachmentEvents24: (facts: {
            arg1: string;
            companyId: string;
            linkId: string;
            setId: string;
            userId: string;
            arg6: "file.reserved";
            arg7: string;
            now: number;
        }) => { return nativeWrite('SR_007', [facts.arg1, facts.companyId, facts.linkId, facts.setId, facts.userId, facts.arg6, facts.arg7, facts.now]); },
        readUploadIntents25: (facts: {
            uploadIntentId: string;
            userId: string;
            companyId: string;
            arg4: number;
        }) => {
            return db.prepare(`${uploadIntentForSetSql}
      AND (status IN ('uploaded','ready') OR (status='pending' AND expires_at>?))`).bind(facts.uploadIntentId, facts.userId, facts.companyId, facts.arg4).first<Record<string, unknown>>();
        },
        readAttachments26: (facts: {
            attachmentId: string;
            companyId: string;
        }) => { return db.prepare(attachmentChecksumSql).bind(facts.attachmentId, facts.companyId).first<{
            checksum_sha256: string;
            source_state: AttachmentSourceState;
        }>(); },
        lock27: (facts: {
            arg1: string;
        }) => { return db.prepare(advisoryLockSql).bind(facts.arg1).run(); },
        readUploadIntents28: (facts: {
            uploadIntentId: string;
            userId: string;
            companyId: string;
        }) => { return db.prepare(`${uploadIntentForSetSql} FOR UPDATE`).bind(facts.uploadIntentId, facts.userId, facts.companyId).first<Record<string, unknown>>(); },
        readAttachments29: (facts: {
            attachmentId: string;
            companyId: string;
        }) => { return db.prepare(attachmentChecksumSql).bind(facts.attachmentId, facts.companyId).first<{
            checksum_sha256: string;
            source_state: AttachmentSourceState;
        }>(); },
        readAttachmentLinks30: (facts: {
            attachment_link_id: unknown;
            attachment_set_id: unknown;
            companyId: string;
        }) => {
            return db.prepare(`SELECT l.attachment_id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
            WHERE l.id=? AND l.set_id=? AND s.company_id=?`).bind(facts.attachment_link_id, facts.attachment_set_id, facts.companyId).first<{
                attachment_id: string | null;
            }>();
        },
        readAttachmentLinks31: (facts: {
            attachment_link_id: unknown;
            attachment_set_id: unknown;
            companyId: string;
        }) => {
            return db.prepare(`SELECT l.state,l.set_id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
          WHERE l.id=? AND l.set_id=? AND s.company_id=? FOR UPDATE`).bind(facts.attachment_link_id, facts.attachment_set_id, facts.companyId).first<{
                state: string;
                set_id: string;
            }>();
        },
        readAttachmentSets32: (facts: {
            attachment_set_id: unknown;
            companyId: string;
        }) => { return db.prepare(setLockSql).bind(facts.attachment_set_id, facts.companyId).first<{
            state: string;
        }>(); },
        stepAttachments33: (facts: {
            attachmentId: string;
            uploadIntentId: string;
            owner_type: unknown;
            owner_id: unknown;
            purpose: unknown;
            objectKey: string;
            revision: unknown;
            original_name: unknown;
            media_type: unknown;
            byteLength: number;
            checksum: string;
            userId: string;
            now: number;
            companyId: string;
            detected: "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/pdf" | "text/csv" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
        }) => {
            return nativeWrite('SR_008', [facts.attachmentId, facts.uploadIntentId, facts.owner_type, facts.owner_id, facts.purpose, facts.objectKey, facts.revision, facts.original_name, facts.media_type, facts.byteLength, facts.checksum, facts.userId, facts.now, facts.companyId, facts.detected]);
        },
        stepUploadIntents34: (facts: {
            byteLength: number;
            checksum: string;
            detected: "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/pdf" | "text/csv" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
            objectKey: string;
            now: number;
            uploadIntentId: string;
            companyId: string;
        }) => { return nativeWrite('SR_009', [facts.byteLength, facts.checksum, facts.detected, facts.objectKey, facts.now, facts.uploadIntentId, facts.companyId]); },
        stepAttachmentLinks35: (facts: {
            attachmentId: string;
            attachment_link_id: unknown;
            companyId: string;
            ownerType: unknown;
        }) => {
            const kind = facts.ownerType === "leave_request" ? 'SR_057' : 'SR_056';
            return nativeWrite(kind, [facts.attachmentId, facts.attachment_link_id, facts.companyId]);
        },
        stepAttachmentScanAttempts36: (facts: {
            arg1: string;
            attachmentId: string;
            now: number;
        }) => { return nativeWrite('SR_010', [facts.arg1, facts.attachmentId, facts.now]); },
        stepAttachmentStorageLedger37: (facts: {
            arg1: string;
            companyId: string;
            attachmentId: string;
            objectKey: string;
            byteLength: number;
            now: number;
        }) => { return nativeWrite('SR_011', [facts.arg1, facts.companyId, facts.attachmentId, facts.objectKey, facts.byteLength, facts.now]); },
        stepAttachmentEvents38: (facts: {
            arg1: string;
            companyId: string;
            attachmentId: string;
            attachment_link_id: unknown;
            attachment_set_id: unknown;
            userId: string;
            arg7: "file.quarantined";
            arg8: string;
            now: number;
        }) => { return nativeWrite('SR_012', [facts.arg1, facts.companyId, facts.attachmentId, facts.attachment_link_id, facts.attachment_set_id, facts.userId, facts.arg7, facts.arg8, facts.now]); },
        readAttachmentLinks39: (facts: {
            linkId: string;
            companyId: string;
        }) => {
            return db.prepare(`${ATTACHMENT_LINK_PROJECTION_HEAD_SQL}
    a.source_state,a.id attachment_id,a.preview_object_key FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id
      AND a.company_id=s.company_id
    WHERE l.id=? AND s.company_id=? AND (l.attachment_id IS NULL OR a.id IS NOT NULL)`).bind(facts.linkId, facts.companyId).first<Record<string, unknown>>();
        },
        readAttachmentScanAttempts40: (facts: {
            attachment_id: unknown;
        }) => { return db.prepare("SELECT attempt,state,result_code,error_code,scanner_signature,scanner_definition_at,completed_at FROM attachment_scan_attempts WHERE attachment_id=? ORDER BY attempt DESC LIMIT 1").bind(facts.attachment_id).first<Record<string, unknown>>(); },
        writeAttachmentAccessEvents41: (facts: {
            arg1: string;
            companyId: string;
            attachment_id: unknown;
            linkId: string;
            userId: string;
            arg6: number;
        }) => { return nativeWrite('SR_013', [facts.arg1, facts.companyId, facts.attachment_id, facts.linkId, facts.userId, facts.arg6, facts.attachment_id]).run(); },
        readAttachmentLinks42: (facts: {
            linkId: string;
            companyId: string;
        }) => {
            return db.prepare(`SELECT l.state link_state,l.created_by link_creator,s.target_type,s.target_id,s.component_kind,s.purpose_key,
    a.id attachment_id,a.object_key,a.preview_object_key,a.preview_media_type,a.original_name,a.media_type,a.source_state
    FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
    WHERE l.id=? AND s.company_id=?`).bind(facts.linkId, facts.companyId).first<Record<string, unknown>>();
        },
        writeAttachmentAccessEvents43: (facts: {
            arg1: string;
            companyId: string;
            attachment_id: unknown;
            linkId: string;
            userId: string;
            variant: "original" | "preview";
            arg7: number;
        }) => { return nativeWrite('SR_014', [facts.arg1, facts.companyId, facts.attachment_id, facts.linkId, facts.userId, facts.variant, facts.arg7]).run(); },
        writeAttachmentAccessEvents44: (facts: {
            arg1: string;
            companyId: string;
            attachment_id: unknown;
            linkId: string;
            userId: string;
            variant: "original" | "preview";
            arg7: string;
            arg8: number;
        }) => { return nativeWrite('SR_015', [facts.arg1, facts.companyId, facts.attachment_id, facts.linkId, facts.userId, facts.variant, facts.arg7, facts.arg8]).run(); },
        readAttachmentLinks45: (facts: {
            linkId: string;
            companyId: string;
        }) => { return db.prepare(attachmentLinkSql).bind(facts.linkId, facts.companyId).first<Record<string, unknown>>(); },
        readAttachmentLinks46: (facts: {
            linkId: string;
            companyId: string;
        }) => { return db.prepare(`${attachmentLinkSql} FOR UPDATE`).bind(facts.linkId, facts.companyId).first<Record<string, unknown>>(); },
        lock49: (facts: {
            arg1: string;
        }) => { return db.prepare(advisoryLockSql).bind(facts.arg1).run(); },
        readAttachmentEvents50: (facts: {
            eventId: string;
            companyId: string;
            userId: string;
        }) => { return db.prepare("SELECT set_id,details_json FROM attachment_events WHERE id=? AND company_id=? AND actor_id=? AND action='sanitization.attested'").bind(facts.eventId, facts.companyId, facts.userId).first<{
            set_id: string | null;
            details_json: string;
        }>(); },
        readAttachmentSets51: (facts: {
            setId: string;
            companyId: string;
        }) => { return db.prepare(setLockSql).bind(facts.setId, facts.companyId).first<{
            state: string;
        }>(); },
        readAttachmentLinks52: (facts: {
            setId: string;
            companyId: string;
        }) => {
            return db.prepare(`SELECT l.id,a.id source_id,a.original_name,a.checksum_sha256,a.media_type,a.bytes,a.source_state,l.caption
      FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
      WHERE l.set_id=? AND s.company_id=? AND l.state='draft' ORDER BY l.created_at,l.id`).bind(facts.setId, facts.companyId).all<{
                id: string;
                source_id: string | null;
                original_name: string | null;
                checksum_sha256: string | null;
                media_type: string | null;
                bytes: number | null;
                source_state: AttachmentSourceState | null;
                caption: string | null;
            }>();
        },
        stepAttachmentEvents53: (facts: {
            eventId: string;
            companyId: string;
            setId: string;
            userId: string;
            arg5: "sanitization.attested";
            details: string;
            now: number;
        }) => { return nativeWrite('SR_016', [facts.eventId, facts.companyId, facts.setId, facts.userId, facts.arg5, facts.details, facts.now]); },
        stepAttachmentSets54: (facts: {
            now: number;
            setId: string;
            companyId: string;
        }) => { return nativeWrite('SR_017', [facts.now, facts.setId, facts.companyId]); },
        readAttachmentLinks55: (facts: {
            sourceLinkId: string;
            companyId: string;
        }) => {
            return db.prepare(`SELECT l.*,s.company_id,s.target_type,s.target_id,s.component_kind,s.purpose_key,s.classification,s.audience_json,a.source_state,a.media_type,a.bytes
    FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id WHERE l.id=? AND s.company_id=? AND a.company_id=s.company_id`).bind(facts.sourceLinkId, facts.companyId).first<Record<string, unknown>>();
        },
        lock56: (facts: {
            arg1: string;
        }) => { return db.prepare(advisoryLockSql).bind(facts.arg1).run(); },
        readAttachmentSets57: (facts: {
            setId: string;
            companyId: string;
        }) => { return db.prepare(`${setByIdSql} FOR UPDATE`).bind(facts.setId, facts.companyId).first<SetRow>(); },
        readAttachmentLinks58: (facts: {
            id: string;
            companyId: string;
        }) => { return db.prepare("SELECT l.set_id,l.attachment_id,l.caption FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=? AND s.company_id=?").bind(facts.id, facts.companyId).first<{
            set_id: string;
            attachment_id: string;
            caption: string | null;
        }>(); },
        readAttachmentLinks59: (facts: {
            setId: string;
            attachment_id: unknown;
            companyId: string;
        }) => { return db.prepare(`${LINK_SET_ID_SELECT_SQL} WHERE l.set_id=? AND l.attachment_id=? AND s.company_id=? AND l.state IN ('draft','bound')`).bind(facts.setId, facts.attachment_id, facts.companyId).first<{
            id: string;
        }>(); },
        readAttachmentLinks60: (facts: {
            setId: string;
            companyId: string;
        }) => { return db.prepare(SET_DRAFT_BOUND_COUNT_SQL(false)).bind(facts.setId, facts.companyId).first("count"); },
        stepAttachmentLinks61: (facts: {
            id: string;
            setId: string;
            attachment_id: unknown;
            caption: string | null;
            userId: string;
            now: number;
            arg7: number;
        }) => { return nativeWrite('SR_018', [facts.id, facts.setId, facts.attachment_id, facts.caption, facts.userId, facts.now, facts.arg7]); },
        stepAttachmentEvents62: (facts: {
            arg1: string;
            companyId: string;
            attachment_id: unknown;
            id: string;
            setId: string;
            userId: string;
            arg7: "link.reused";
            arg8: string;
            now: number;
        }) => { return nativeWrite('SR_019', [facts.arg1, facts.companyId, facts.attachment_id, facts.id, facts.setId, facts.userId, facts.arg7, facts.arg8, facts.now]); },
        readAttachmentSets63: (facts: {
            companyId: string;
            setIds: (string)[];
        }) => {
            return db.prepare(`SELECT count(DISTINCT candidate.id) FROM attachment_sets candidate
    WHERE candidate.company_id=? AND candidate.id IN (${facts.setIds.map(() => "?").join(",")})
      AND ${setCreatedCompatibilitySql('true')}`).bind(facts.companyId, ...facts.setIds).first("count");
        },
        readAttachmentLinks64: (facts: {
            id: string;
            companyId: string;
        }) => {
            return db.prepare(`${LINK_ATTACHMENT_SELECT_SQL},a.checksum_sha256,a.media_type,a.bytes,a.source_state
      FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
      WHERE l.set_id=? AND s.company_id=? AND l.state='draft' ORDER BY l.created_at,l.id`).bind(facts.id, facts.companyId).all<{
                link_id: string;
                attachment_id: string | null;
                checksum_sha256: string | null;
                media_type: string | null;
                bytes: number | null;
                source_state: AttachmentSourceState | null;
            }>();
        },
        readAttachmentEvents65: (facts: {
            id: string;
            companyId: string;
        }) => {
            return db.prepare(sanitizationAttestedSql).bind(facts.id, facts.companyId).first();
        },
        stepAttachmentLinks66: (facts: {
            now: number;
            bindingType: string;
            bindingId: string;
            link_id: string;
            companyId: string;
        }) => { return nativeWrite('SR_058', [facts.now, facts.bindingType, facts.bindingId, facts.link_id, facts.companyId]); },
        stepAttachmentSets67: (facts: {
            now: number;
            commandId: string;
            id: string;
            companyId: string;
        }) => { return nativeWrite('SR_020', [facts.now, facts.commandId, facts.now, facts.id, facts.companyId]); },
        stepAttachmentEvents68: (facts: {
            arg1: string;
            companyId: string;
            id: string;
            userId: string;
            arg5: "set.bound";
            arg6: string;
            now: number;
        }) => { return nativeWrite('SR_021', [facts.arg1, facts.companyId, facts.id, facts.userId, facts.arg5, facts.arg6, facts.now]); },
        readAttachmentLinks75: (facts: {
            sourceLinkId: string;
            companyId: string;
        }) => {
            return db.prepare(`${LINK_ATTACHMENT_SELECT_SQL},s.target_type source_target_type,s.target_id source_target_id,s.component_kind,s.purpose_key,s.company_id,
    a.preview_object_key,a.preview_media_type,a.preview_checksum_sha256,a.preview_bytes,a.source_state
    FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
    WHERE l.id=? AND s.company_id=? AND l.state='bound'`).bind(facts.sourceLinkId, facts.companyId).first<Record<string, unknown>>();
        },
        readAttachmentPublications76: (facts: {
            id: string;
            companyId: string;
        }) => { return db.prepare(publicationByIdSql).bind(facts.id, facts.companyId).first<Record<string, unknown>>(); },
        lock77: (facts: {
            arg1: string;
        }) => { return db.prepare(advisoryLockSql).bind(facts.arg1).run(); },
        readAttachmentPublications78: (facts: {
            id: string;
            companyId: string;
        }) => { return db.prepare(publicationByIdSql).bind(facts.id, facts.companyId).first<Record<string, unknown>>(); },
        readAttachmentPublications79: (facts: {
            companyId: string;
            targetType: string;
            targetId: string;
        }) => { return db.prepare("SELECT COALESCE(max(revision),0)+1 AS next_revision FROM attachment_publications WHERE company_id=? AND target_type=? AND target_id=?").bind(facts.companyId, facts.targetType, facts.targetId).first("next_revision"); },
        stepAttachmentPublications80: (facts: {
            id: string;
            companyId: string;
            sourceLinkId: string;
            targetType: string;
            targetId: string;
            key: string;
            preview_media_type: unknown;
            checksum: string;
            byteLength: number;
            alt: string;
            ordinal: number;
            revision: number;
            userId: string;
            now: number;
        }) => {
            return nativeWrite('SR_022', [facts.id, facts.companyId, facts.sourceLinkId, facts.targetType, facts.targetId, facts.key, facts.preview_media_type, facts.checksum, facts.byteLength, facts.alt, facts.ordinal, facts.revision, facts.userId, facts.now]);
        },
        stepAttachmentPublicationOrders81: (facts: {
            companyId: string;
            targetType: string;
            targetId: string;
            userId: string;
            now: number;
        }) => { return nativeWrite('SR_023', [facts.companyId, facts.targetType, facts.targetId, facts.userId, facts.now]); },
        stepAttachmentStorageLedger82: (facts: {
            arg1: string;
            companyId: string;
            attachment_id: unknown;
            key: string;
            byteLength: number;
            now: number;
        }) => { return nativeWrite('SR_024', [facts.arg1, facts.companyId, facts.attachment_id, facts.key, facts.byteLength, facts.now]); },
        stepAttachmentEvents83: (facts: {
            arg1: string;
            companyId: string;
            attachment_id: unknown;
            sourceLinkId: string;
            userId: string;
            arg6: "publication.created";
            arg7: string;
            now: number;
        }) => { return nativeWrite('SR_025', [facts.arg1, facts.companyId, facts.attachment_id, facts.sourceLinkId, facts.userId, facts.arg6, facts.arg7, facts.now]); },
        readAttachmentPublications84: (facts: {
            id: string;
        }) => { return db.prepare(attachmentPublicationByIdSql(false)).bind(facts.id).first<Record<string, unknown>>(); },
        readAttachmentPublications85: (facts: {
            companyId: string;
            targetType: string;
            targetId: string;
            canManage: boolean;
        }) => {
            return db.prepare(`SELECT id,source_link_id,target_type,target_id,derivative_media_type,derivative_checksum_sha256,derivative_bytes,alt_text,ordinal,revision,state,created_by,created_at,approved_by,approved_at,revoked_by,revoked_at,revocation_reason FROM attachment_publications publication WHERE company_id=? AND target_type=? AND target_id=?
    AND EXISTS(${LINK_SET_JOIN_SQL} set_row ON set_row.id=link.set_id WHERE link.id=publication.source_link_id AND set_row.company_id=publication.company_id)
    ${facts.canManage ? "" : "AND state='approved'"} ORDER BY ordinal,revision,id`).bind(facts.companyId, facts.targetType, facts.targetId).all<Record<string, unknown>>();
        },
        readAttachmentPublicationOrders86: (facts: {
            companyId: string;
            targetType: string;
            targetId: string;
        }) => { return db.prepare(publicationOrderVersionSql).bind(facts.companyId, facts.targetType, facts.targetId).first<{
            version: number;
        }>(); },
        readPublicationRevocation: (facts:{eventId:string;companyId:string;userId:string;publicationId:string;reason:string;}) => db.prepare(`SELECT event.id FROM attachment_events event JOIN attachment_publications publication
          ON publication.id=? AND publication.company_id=event.company_id AND publication.source_link_id=event.link_id
          AND publication.state='revoked' AND publication.revoked_by=event.actor_id AND publication.revocation_reason=?
          WHERE event.id=? AND event.company_id=? AND event.actor_id=? AND event.action='publication.revoked'
          AND event.details_json::jsonb->>'publicationId'=publication.id AND event.details_json::jsonb->>'reason'=publication.revocation_reason`)
          .bind(facts.publicationId,facts.reason,facts.eventId,facts.companyId,facts.userId).first<{id:string}>(),
        readAttachmentPublications87: (facts: {
            id: string;
            companyId: string;
        }) => {
            return db.prepare(`SELECT p.*,s.component_kind,s.purpose_key,s.target_type source_target_type,s.target_id source_target_id
    FROM attachment_publications p JOIN attachment_links l ON l.id=p.source_link_id JOIN attachment_sets s ON s.id=l.set_id
      AND s.company_id=p.company_id
    WHERE p.id=? AND p.company_id=?`).bind(facts.id, facts.companyId).first<Record<string, unknown>>();
        },
        readAttachmentPublications88: (facts: {
            id: string;
            companyId: string;
        }) => { return db.prepare(`${publicationByIdSql} FOR UPDATE`).bind(facts.id, facts.companyId).first<Record<string, unknown>>(); },
        stepAttachmentPublications89: (facts: {
            userId: string;
            now: number;
            text: string;
            id: string;
            companyId: string;
        }) => { return nativeWrite('SR_026', [facts.userId, facts.now, facts.text, facts.id, facts.companyId]); },
        stepAttachmentEvents90: (facts: {
            arg1: string;
            companyId: string;
            source_link_id: unknown;
            userId: string;
            arg5: "publication.revoked";
            arg6: string;
            now: number;
        }) => { return nativeWrite('SR_027', [facts.arg1, facts.companyId, facts.source_link_id, facts.userId, facts.arg5, facts.arg6, facts.now]); },
        lock92: (facts: {
            arg1: string;
        }) => { return db.prepare(advisoryLockSql).bind(facts.arg1).run(); },
        readAttachmentEvents93: (facts: {
            eventId: string;
            companyId: string;
            userId: string;
        }) => { return db.prepare("SELECT details_json FROM attachment_events WHERE id=? AND company_id=? AND actor_id=? AND action='publication.reordered'").bind(facts.eventId, facts.companyId, facts.userId).first<{
            details_json: string;
        }>(); },
        readAttachmentPublicationOrders94: (facts: {
            companyId: string;
            targetType: string;
            targetId: string;
        }) => { return db.prepare(`${publicationOrderVersionSql} FOR UPDATE`).bind(facts.companyId, facts.targetType, facts.targetId).first<{
            version: number;
        }>(); },
        readAttachmentPublications95: (facts: {
            companyId: string;
            targetType: string;
            targetId: string;
        }) => { return db.prepare("SELECT id FROM attachment_publications WHERE company_id=? AND target_type=? AND target_id=? AND state<>'revoked' ORDER BY id").bind(facts.companyId, facts.targetType, facts.targetId).all<{
            id: string;
        }>(); },
        stepAttachmentPublications96: (facts: {
            ordinal: number;
            id: string;
            companyId: string;
        }) => { return nativeWrite('SR_028', [facts.ordinal, facts.id, facts.companyId]); },
        stepAttachmentPublicationOrders97: (facts: {
            companyId: string;
            targetType: string;
            targetId: string;
            userId: string;
            now: number;
            expectedVersion: number;
        }) => { return nativeWrite('SR_029', [facts.companyId, facts.targetType, facts.targetId, facts.userId, facts.now, facts.expectedVersion]); },
        stepAttachmentEvents98: (facts: {
            eventId: string;
            companyId: string;
            userId: string;
            arg4: "publication.reordered";
            arg5: string;
            now: number;
        }) => { return nativeWrite('SR_030', [facts.eventId, facts.companyId, facts.userId, facts.arg4, facts.arg5, facts.now]); },
        readAttachmentPublications99: (facts: {
            id: string;
            companyId: string;
        }) => {
            return db.prepare(`SELECT p.*,s.target_type source_target_type,s.target_id source_target_id,s.component_kind,s.purpose_key
    FROM attachment_publications p JOIN attachment_links l ON l.id=p.source_link_id JOIN attachment_sets s ON s.id=l.set_id AND s.company_id=p.company_id
    WHERE p.id=? AND p.company_id=?`).bind(facts.id, facts.companyId).first<Record<string, unknown>>();
        },
        readAttachmentPublications100: (facts: {
            id: string;
            companyId: string;
        }) => { return db.prepare("SELECT state FROM attachment_publications WHERE id=? AND company_id=? FOR UPDATE").bind(facts.id, facts.companyId).first<{
            state: string;
        }>(); },
        stepAttachmentPublications102: (facts: {
            userId: string;
            now: number;
            id: string;
            companyId: string;
        }) => { return nativeWrite('SR_031', [facts.userId, facts.now, facts.id, facts.companyId]); },
        stepAttachmentEvents103: (facts: {
            arg1: string;
            companyId: string;
            source_link_id: unknown;
            userId: string;
            arg5: "publication.approved";
            arg6: string;
            now: number;
        }) => { return nativeWrite('SR_032', [facts.arg1, facts.companyId, facts.source_link_id, facts.userId, facts.arg5, facts.arg6, facts.now]); },
        readAttachmentPublications105: (facts: {
            id: string;
        }) => { return db.prepare("SELECT derivative_object_key,derivative_media_type,derivative_checksum_sha256,alt_text,revision FROM attachment_publications WHERE id=? AND state='approved'").bind(facts.id).first<{
            derivative_object_key: string;
            derivative_media_type: string;
            derivative_checksum_sha256: string;
            alt_text: string;
            revision: number;
        }>(); },
        legacyScanGuard: (facts: {sourceId:string;jobId:string;lease:string}) => db.prepare('SELECT erpx_files_assert_legacy_scan(?,?,?)').bind(facts.sourceId,facts.jobId,facts.lease),
        legacyScanMark: (facts: {sourceId:string}) => nativeWrite('SR_033', [facts.sourceId]),
        readAttachmentScanAttempts106: (facts: {
            now: number;
            arg2: number;
            lease: string;
            arg4: string | null;
            arg5: string | null;
        }) => {
            return db.prepare('SELECT * FROM erpx_files_claim_legacy_scan(?,?,?,?)').bind(facts.now, facts.arg2, facts.lease, facts.arg4).first<Record<string, unknown>>();
        },
        readAttachments107: (facts: {
            attachment_id: unknown;
        }) => { return db.prepare(ATTACHMENT_STAR_ID_HEAD_SQL).bind(facts.attachment_id).first<Record<string, unknown>>(); },
        stepAttachmentScanAttempts109: (facts: {
            engine: string;
            signature: string | null;
            definitionUpdatedAt: number;
            arg4: number;
            id: unknown;
            lease: string;
        }) => { return nativeWrite('SR_034', [facts.engine, facts.signature, facts.definitionUpdatedAt, facts.arg4, facts.id, facts.lease]); },
        stepAttachments110: (facts: {
            detected: "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/pdf" | "text/csv" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
            arg2: number;
            definitionUpdatedAt: number;
            id: unknown;
        }) => { return nativeWrite('SR_035', [facts.detected, facts.arg2, facts.definitionUpdatedAt, facts.id]); },
        stepAttachmentEvents111: (facts: {
            arg1: string;
            company_id: unknown;
            id: unknown;
            arg4: "system:jobs";
            arg5: "scan.malware_rejected";
            arg6: string;
            arg7: number;
        }) => { return nativeWrite('SR_036', [facts.arg1, facts.company_id, facts.id, facts.arg4, facts.arg5, facts.arg6, facts.arg7]); },
        stepAttachmentScanAttempts112: (facts: {
            engine: string;
            definitionUpdatedAt: number;
            completed: number;
            id: unknown;
            lease: string;
        }) => { return nativeWrite('SR_037', [facts.engine, facts.definitionUpdatedAt, facts.completed, facts.id, facts.lease]); },
        stepAttachments113: (facts: {
            detected: "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/pdf" | "text/csv" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
            completed: number;
            definitionUpdatedAt: number;
            previewKey: string | null;
            arg5: "image/jpeg" | "image/webp" | null;
            previewChecksum: string | null;
            arg7: number | null;
            id: unknown;
        }) => { return nativeWrite('SR_038', [facts.detected, facts.completed, facts.definitionUpdatedAt, facts.previewKey, facts.arg5, facts.previewChecksum, facts.arg7, facts.id]); },
        stepUploadIntents114: (facts: {
            completed: number;
            upload_id: unknown;
        }) => { return nativeWrite('SR_039', [facts.completed, facts.upload_id]); },
        stepAttachmentLinks115: (facts: {
            arg1: number;
            upload_id: unknown;
        }) => { return nativeWrite('SR_040', [facts.arg1, facts.upload_id]); },
        stepAttachmentEvents116: (facts: {
            arg1: string;
            company_id: unknown;
            id: unknown;
            arg4: "system:jobs";
            arg5: "scan.available";
            arg6: string;
            completed: number;
        }) => { return nativeWrite('SR_041', [facts.arg1, facts.company_id, facts.id, facts.arg4, facts.arg5, facts.arg6, facts.completed]); },
        stepAttachmentStorageLedger117: (facts: {
            arg1: string;
            company_id: unknown;
            id: unknown;
            previewKey: string;
            byteLength: number;
            completed: number;
        }) => { return nativeWrite('SR_042', [facts.arg1, facts.company_id, facts.id, facts.previewKey, facts.byteLength, facts.completed]); },
        stepAttachmentScanAttempts118: (facts: {
            code: string;
            completed: number;
            id: unknown;
            lease: string;
        }) => { return nativeWrite('SR_043', [facts.code, facts.completed, facts.id, facts.lease]); },
        stepAttachments119: (facts: {
            id: unknown;
        }) => { return nativeWrite('SR_044', [facts.id]); },
        stepAttachmentEvents120: (facts: {
            arg1: string;
            company_id: unknown;
            id: unknown;
            arg4: "system:jobs";
            arg5: "scan.error";
            arg6: string;
            completed: number;
        }) => { return nativeWrite('SR_045', [facts.arg1, facts.company_id, facts.id, facts.arg4, facts.arg5, facts.arg6, facts.completed]); },
        stepAttachmentScanAttempts121: (facts: {
            arg1: string;
            id: unknown;
            arg3: number;
            arg4: number;
        }) => { return nativeWrite('SR_046', [facts.arg1, facts.id, facts.arg3, facts.arg4]); },
        readAttachments122: (facts: {
            attachmentId: string;
        }) => { return db.prepare("SELECT source_state FROM attachments WHERE id=?").bind(facts.attachmentId).first<{
            source_state: AttachmentSourceState;
        }>(); },
        readAttachments123: (facts: {
            attachmentId: string;
        }) => { return db.prepare("SELECT source_state,legal_hold FROM attachments WHERE id=?").bind(facts.attachmentId).first<{
            source_state: string;
            legal_hold: number;
        }>(); },
        readAttachmentLinks124: (facts: {
            now: number;
            attachmentId: string;
        }) => {
            return db.prepare(`SELECT
      count(*) FILTER(WHERE l.state IN ('draft','bound')) active_links,
      count(*) FILTER(WHERE l.state IN ('expired','withdrawn') AND l.expires_at<=?) closed_due_links,
      count(*) FILTER(WHERE s.retention_policy_key IS NOT NULL AND (s.destruction_mode IS DISTINCT FROM 'automatic' OR s.retain_until IS NULL)) review_obligations,
      max(s.retain_until) FILTER(WHERE s.retention_policy_key IS NOT NULL) retain_until
    FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.attachment_id=?`).bind(facts.now, facts.attachmentId).first<{
                active_links: number;
                closed_due_links: number;
                review_obligations: number;
                retain_until: number | null;
            }>();
        },
        readAttachmentSets126: (facts: {
            now: number;
            arg2: number;
        }) => {
            return db.prepare(`SELECT DISTINCT set_row.company_id,set_row.target_id
    FROM attachment_sets set_row
    WHERE set_row.target_type='leave_request' AND set_row.state='draft' AND (
      EXISTS(${SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL} AND link.state='draft' AND link.expires_at<=?)
      OR (NOT EXISTS(${SET_ROW_DRAFT_LINK_EXISTS_HEAD_SQL}) AND set_row.created_at<=?)
    ) ORDER BY set_row.company_id,set_row.target_id`).bind(facts.now, facts.arg2).all<{
                company_id: string;
                target_id: string;
            }>();
        },
        readAttachments138: (facts: {
            now: number;
        }) => {
            return db.prepare(`${SELECT_ATTACHMENT_ID_SQL} WHERE a.company_id IS NOT NULL AND a.legal_hold=0 AND a.source_state='available' AND NOT erpx_files_field_attachment_owned(a.id)
    AND EXISTS(SELECT 1 FROM attachment_links closed WHERE closed.attachment_id=a.id AND closed.state IN ('expired','withdrawn') AND closed.expires_at<=?)
    AND NOT EXISTS(SELECT 1 FROM attachment_links l WHERE l.attachment_id=a.id AND l.state IN ('draft','bound'))
    AND NOT ${LINK_SET_WHERE_EXISTS_SQL}attachment_id=a.id AND s.retention_policy_key IS NOT NULL
      AND (s.destruction_mode IS DISTINCT FROM 'automatic' OR s.retain_until IS NULL OR s.retain_until>?))
    AND NOT EXISTS(SELECT 1 FROM (${legalProjectionSources.legal_file_preservations}) p JOIN (${legalProjectionSources.legal_hold_targets}) target ON target.item_id=p.item_id JOIN (${legalProjectionSources.legal_holds}) hold ON hold.id=target.hold_id
      WHERE p.source_type='attachment' AND p.source_id=a.id AND hold.state<>'released')
    AND NOT EXISTS(SELECT 1 FROM attachment_links legal_link JOIN attachment_sets legal_set ON legal_set.id=legal_link.set_id
      JOIN (${legalProjectionSources.legal_hold_targets}) target ON target.item_id=legal_set.target_id JOIN (${legalProjectionSources.legal_holds}) hold ON hold.id=target.hold_id
      WHERE legal_link.attachment_id=a.id AND legal_set.target_type='legal_item' AND hold.state<>'released') LIMIT 100`).bind(facts.now, facts.now).all<{
                id: string;
            }>();
        },
        lock139: (facts: {
            arg1: string;
        }) => { return db.prepare(advisoryLockSql).bind(facts.arg1).run(); },
        lock140: () => { return db.prepare("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))").bind().run(); },
        readAttachments141: (facts: {
            id: string;
        }) => { return db.prepare("SELECT id FROM attachments WHERE id=? AND source_state='available' FOR UPDATE").bind(facts.id).first<{
            id: string;
        }>(); },
        readAttachments142: (facts: {
            now: number;
            id: string;
        }) => { return nativeWrite('SR_051', [facts.now, facts.id]).first<{
            company_id: string;
        }>(); },
        writeAttachmentStorageLedger143: (facts: {
            now: number;
            id: string;
        }) => { return nativeWrite('SR_052', [facts.now, facts.id]).run(); },
        writeAttachmentEvents144: (facts: {
            arg1: string;
            company_id: string;
            id: string;
            now: number;
        }) => { return nativeWrite('SR_053', [facts.arg1, facts.company_id, facts.id, facts.now]).run(); },
        readAttachments145: () => {
            return db.prepare(`SELECT a.id,a.company_id,a.object_key,a.preview_object_key FROM attachments a
    WHERE a.source_state='destroyed' AND a.legal_hold=0 AND NOT erpx_files_field_attachment_owned(a.id)
      AND NOT EXISTS(SELECT 1 FROM attachment_events e WHERE e.attachment_id=a.id AND e.action='source.bytes_deleted')
    ORDER BY a.destroyed_at,a.id LIMIT 100`).bind().all<{
                id: string;
                company_id: string;
                object_key: string;
                preview_object_key: string | null;
            }>();
        },
        readAttachments148: (facts: {
            id: string;
        }) => { return db.prepare("SELECT company_id,object_key,preview_object_key,source_state,legal_hold FROM attachments WHERE id=? FOR UPDATE").bind(facts.id).first<{
            company_id: string;
            object_key: string;
            preview_object_key: string | null;
            source_state: string;
            legal_hold: number;
        }>(); },
        readAttachmentEvents149: (facts: {
            id: string;
        }) => { return db.prepare(ATTACHMENT_EVENTS_WHERE_HEAD_SQL + "attachment_id=? AND action='source.bytes_deleted'").bind(facts.id).first(); },
        readAttachmentStorageLedger150: (facts: {
            id: string;
        }) => { return db.prepare(`${LEDGER_OBJECT_KEY_PROJECTION_HEAD_SQL}attachment_id=?`).bind(facts.id).all<{
            object_key: string;
        }>(); },
        writeAttachmentEvents151: (facts: {
            eventId: string;
            company_id: string;
            id: string;
            arg4: number;
        }) => { return nativeWrite('SR_054', [facts.eventId, facts.company_id, facts.id, facts.arg4]).run(); },
        writeAttachmentEvents152: (facts: {
            arg1: string;
            id: string;
            arg3: string;
            arg4: number;
        }) => { return nativeWrite('SR_055', [facts.arg1, facts.id, facts.arg3, facts.arg4, facts.id]).run(); },
        bindingGuard1: (facts: {
            commandId: string;
            step: string;
            values: (string | number | null)[];
        }) => assertion(db, facts.commandId, facts.step, `EXISTS(${SANITIZATION_ATTESTED_HEAD_SQL} AND attested.created_at>=COALESCE(${changedCreatedAtSql},0))`, facts.values),
        bindingGuard2: (facts: {
            commandId: string;
            step: string;
            values: (string | number | null)[];
            historicalLeave: boolean;
        }) => assertion(db, facts.commandId, facts.step, `EXISTS(SELECT 1 FROM attachment_sets WHERE id=? AND company_id=? AND target_type=? AND target_id=? AND state='draft' AND expected_target_version${facts.historicalLeave ? "<=" : "="}? AND policy_version=? AND purpose_key=? AND component_kind=? AND component_ref IS NOT DISTINCT FROM ?)`, facts.values),
        bindingGuard3: (facts: {
            commandId: string;
            step: string;
            values: (string | number | null)[];
        }) => assertion(db, facts.commandId, facts.step, `${LINK_SET_ATTACHMENT_EXISTS_SQL} AND a.company_id=s.company_id WHERE l.id=? AND l.set_id=? AND s.company_id=? AND l.state='draft' AND a.source_state='available' AND a.checksum_sha256=?)`, facts.values) };
}
