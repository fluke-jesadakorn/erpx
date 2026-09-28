import "server-only";

import { randomUUID } from "node:crypto";
import type { FileStore, SqlDatabase } from "../runtime/types";
import { PostgresDatabase } from "../runtime/postgres";
import {
  ATTACHMENT_RESERVATION_MS,
  attachmentUploadsFrozen,
  attachmentsV2ReadOnly,
} from "../attachments/feature";
import {
  assertAttachmentMedia,
  attachmentPolicy,
} from "../attachments/policy";
import {
  attachmentPreviewReadiness,
  detectedAttachmentMediaType,
} from "../attachments/preview";
import { attachmentScannerReadiness } from "../attachments/scanner";
import { scanAttachmentNow } from "../attachments/service";
import { ApiError } from "../http";
import { hash } from "./service";

const PORTAL_PURPOSE = "legal_portal_submission";

type PortalScope = {
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  itemId: string;
  itemVersion: number;
  shareId: string;
};

export type PortalAttachment = {
  setId: string;
  linkId: string;
  sourceId: string;
  objectKey: string;
  checksum: string;
  mediaType: string;
  bytes: number;
  expectedTargetVersion: number;
};

/**
 * External Legal shares are authenticated but deliberately have no staff
 * WorkContext. This adapter writes the same canonical lifecycle rows as the
 * staff API after the explicit share has resolved its Company and owner.
 */
export async function stagePortalAttachment(
  db: PostgresDatabase,
  files: FileStore,
  actorId: string,
  scope: PortalScope,
  name: string,
  declaredMediaType: string,
  bytes: Uint8Array,
  commandId: string,
): Promise<PortalAttachment> {
  if(attachmentsV2ReadOnly())throw new ApiError(503,"ATTACHMENTS_READ_ONLY","Attachment intake is disabled during rollback. Existing clean bound evidence remains readable.");
  if(attachmentUploadsFrozen())throw new ApiError(503,"ATTACHMENT_UPLOAD_FREEZE","File intake is briefly paused for storage reconciliation.");
  const runtimeEnvironment = {...process.env, ERPX_ATTACHMENTS_V2: "true"};
  const [scanner, preview] = await Promise.all([
    attachmentScannerReadiness(runtimeEnvironment),
    attachmentPreviewReadiness(runtimeEnvironment),
  ]);
  if (!scanner.enabled || !scanner.ready || !preview.ready)
    throw new ApiError(
      503,
      "ATTACHMENT_SCANNER_UNAVAILABLE",
      "File intake is unavailable until the malware scanner, current signatures, and preview tools are ready.",
    );
  const policy = attachmentPolicy("legal_item", PORTAL_PURPOSE, "action");
  assertAttachmentMedia(policy, declaredMediaType, bytes.byteLength);
  const mediaType = detectedAttachmentMediaType(bytes, declaredMediaType);
  if (!mediaType || mediaType !== declaredMediaType)
    throw new ApiError(
      422,
      "LEGAL_UPLOAD_TYPE",
      "The file contents do not match the selected type.",
    );

  const checksum = hash(bytes);
  const setId = `attachment-set-legal-portal-${commandId}`;
  const linkId = `attachment-link-legal-portal-${commandId}`;
  const uploadId = `attachment-upload-legal-portal-${commandId}`;
  const sourceId = `attachment-${uploadId}`;
  const objectKey = `legal/${scope.companyId}/${scope.itemId}/quarantine/${uploadId}/original`;
  const existing = await db
    .prepare(
      "SELECT object_key,checksum_sha256,media_type,bytes FROM attachments WHERE id=? AND company_id=?",
    )
    .bind(sourceId, scope.companyId)
    .first<{
      object_key: string;
      checksum_sha256: string;
      media_type: string;
      bytes: number;
    }>();
  if (
    existing &&
    (existing.checksum_sha256 !== checksum ||
      existing.media_type !== mediaType ||
      Number(existing.bytes) !== bytes.byteLength)
  )
    throw new ApiError(
      409,
      "IDEMPOTENCY_CONFLICT",
      "This upload key was already used for another file.",
    );

  if (!existing) {
    const used = Number(
      await db
        .prepare(
          "SELECT COALESCE(sum(bytes),0) FROM attachment_storage_ledger WHERE company_id=? AND state IN ('reserved','stored')",
        )
        .bind(scope.companyId)
        .first("coalesce"),
    );
    const pending = Number(
      await db
        .prepare(
          "SELECT COALESCE(sum(declared_bytes),0) FROM upload_intents WHERE company_id=? AND status='pending' AND expires_at>?",
        )
        .bind(scope.companyId, Date.now())
        .first("coalesce"),
    );
    const limit = Number(
      (
        await db
          .prepare("SELECT storage_bytes FROM beta_policy WHERE company_id=?")
          .bind(scope.companyId)
          .first<{ storage_bytes: number }>()
      )?.storage_bytes ?? Number.MAX_SAFE_INTEGER,
    );
    if (used + pending + bytes.byteLength > limit)
      throw new ApiError(
        422,
        "STORAGE_QUOTA",
        "Company file storage is full.",
      );

    const stored = await files.put(objectKey, bytes, {
      onlyIf: { etagDoesNotMatch: "*" },
      httpMetadata: {
        contentType: mediaType,
        cacheControl: "private, no-store",
      },
      customMetadata: { sha256: checksum, state: "quarantined" },
    });
    if (!stored) {
      const prior = await files.head(objectKey);
      if (prior?.customMetadata.sha256 !== checksum)
        throw new ApiError(
          409,
          "UPLOAD_CONFLICT",
          "This upload key already contains different bytes.",
        );
    }

    const now = Date.now();
    await db.transaction(async (tx) => {
      const raced = await tx
        .prepare("SELECT checksum_sha256 FROM attachments WHERE id=?")
        .bind(sourceId)
        .first<{ checksum_sha256: string }>();
      if (raced) {
        if (raced.checksum_sha256 !== checksum)
          throw new ApiError(
            409,
            "IDEMPOTENCY_CONFLICT",
            "This upload key was already used for another file.",
          );
        return;
      }
      await tx.batch([
        tx
          .prepare(
            `INSERT INTO attachment_sets(
              id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,
              expected_target_version,policy_version,classification,audience_json,retention_policy_key,
              required_count,max_files,state,created_by,created_at,updated_at
            ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'draft',?,?,?)`,
          )
          .bind(
            setId,
            scope.companyId,
            scope.branchId,
            scope.departmentId,
            "legal_item",
            scope.itemId,
            "action",
            commandId,
            PORTAL_PURPOSE,
            scope.itemVersion,
            policy.version,
            policy.classification,
            JSON.stringify({
              classification: policy.classification,
              portalShareId: scope.shareId,
              policyVersion: policy.version,
            }),
            policy.retentionPolicyKey,
            policy.requiredCount,
            policy.maxFiles,
            actorId,
            now,
            now,
          ),
        tx
          .prepare(
            "INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES(?,?,'draft',?,?,?)",
          )
          .bind(linkId, setId, actorId, now, now + ATTACHMENT_RESERVATION_MS),
        tx
          .prepare(
            `INSERT INTO upload_intents(
              id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,
              staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,
              attachment_set_id,attachment_link_id,company_id,detected_media_type,quarantine_key
            ) VALUES(?,?,?,?,?,?,?,?,?,?,?,'uploaded',?,?,?,?,?,?,?,?,?,?)`,
          )
          .bind(
            uploadId,
            "legal_item",
            scope.itemId,
            PORTAL_PURPOSE,
            name,
            mediaType,
            bytes.byteLength,
            bytes.byteLength,
            checksum,
            objectKey,
            objectKey,
            actorId,
            now + ATTACHMENT_RESERVATION_MS,
            now,
            now,
            scope.itemVersion,
            setId,
            linkId,
            scope.companyId,
            mediaType,
            objectKey,
          ),
        tx
          .prepare(
            `INSERT INTO attachments(
              id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,
              checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state
            ) VALUES(?,?,?,?,?,?,1,?,?,?,?,?,?,?,?,'quarantined')`,
          )
          .bind(
            sourceId,
            uploadId,
            "legal_item",
            scope.itemId,
            PORTAL_PURPOSE,
            objectKey,
            name,
            mediaType,
            bytes.byteLength,
            checksum,
            actorId,
            now,
            scope.companyId,
            mediaType,
          ),
        tx
          .prepare(
            "UPDATE attachment_links SET upload_intent_id=?,attachment_id=? WHERE id=?",
          )
          .bind(uploadId, sourceId, linkId),
        tx
          .prepare(
            "INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at) VALUES(?,?,1,'pending',?)",
          )
          .bind(`attachment-scan-${randomUUID()}`, sourceId, now),
        tx
          .prepare(
            "INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES(?,?,?,'original',?,?,'stored',?)",
          )
          .bind(
            `attachment-storage-${randomUUID()}`,
            scope.companyId,
            sourceId,
            objectKey,
            bytes.byteLength,
            now,
          ),
        tx
          .prepare(
            "INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            `attachment-event-${randomUUID()}`,
            scope.companyId,
            sourceId,
            linkId,
            setId,
            actorId,
            "file.quarantined",
            JSON.stringify({
              checksum,
              bytes: bytes.byteLength,
              detectedMediaType: mediaType,
              portalShareId: scope.shareId,
            }),
            now,
          ),
      ]);
    });
  }

  const state = await scanAttachmentNow(db, files, sourceId);
  if (state !== "available")
    throw new ApiError(
      state === "malware_rejected" ? 422 : 503,
      state === "malware_rejected"
        ? "LEGAL_UPLOAD_REJECTED"
        : "LEGAL_SCAN_UNAVAILABLE",
      state === "malware_rejected"
        ? "The upload was rejected by malware scanning."
        : "The upload could not pass malware scanning.",
    );
  return {
    setId,
    linkId,
    sourceId,
    objectKey,
    checksum,
    mediaType,
    bytes: bytes.byteLength,
    expectedTargetVersion: scope.itemVersion,
  };
}

export async function portalAttachmentBytes(
  db: SqlDatabase,
  files: FileStore,
  input: {
    companyId: string;
    itemId: string;
    linkId: string;
    actorId: string;
    purposeKey: "legal_document";
  },
) {
  const row = await db
    .prepare(
      `SELECT l.id AS link_id,l.attachment_id,a.object_key,a.media_type,a.original_name,a.checksum_sha256,a.source_state
       FROM attachment_links l
       JOIN attachment_sets s ON s.id=l.set_id
       JOIN attachments a ON a.id=l.attachment_id
       WHERE l.id=? AND l.state='bound' AND s.company_id=? AND s.target_type='legal_item'
         AND s.target_id=? AND s.purpose_key=? AND a.source_state='available'`,
    )
    .bind(input.linkId, input.companyId, input.itemId, input.purposeKey)
    .first<{
      link_id: string;
      attachment_id: string;
      object_key: string;
      media_type: string;
      original_name: string;
      checksum_sha256: string;
      source_state: string;
    }>();
  if (!row)
    throw new ApiError(
      404,
      "ATTACHMENT_LINK_NOT_FOUND",
      "File link not found.",
    );
  try {
    const object = await files.get(row.object_key);
    if (!object)
      throw new ApiError(
        503,
        "FILE_UNAVAILABLE",
        "The private file is temporarily unavailable.",
      );
    const bytes = new Uint8Array(await object.arrayBuffer());
    if (hash(bytes) !== row.checksum_sha256)
      throw new ApiError(
        409,
        "LEGAL_INTEGRITY",
        "Document integrity check failed.",
      );
    await db
      .prepare(
        "INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,created_at) VALUES(?,?,?,?,?,'original','allowed',?)",
      )
      .bind(
        `attachment-access-${randomUUID()}`,
        input.companyId,
        row.attachment_id,
        row.link_id,
        input.actorId,
        Date.now(),
      )
      .run();
    return { bytes, mediaType: row.media_type, filename: row.original_name };
  } catch (error) {
    await db
      .prepare(
        "INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,reason,created_at) VALUES(?,?,?,?,?,'original','denied',?,?)",
      )
      .bind(
        `attachment-access-${randomUUID()}`,
        input.companyId,
        row.attachment_id,
        row.link_id,
        input.actorId,
        error instanceof ApiError ? error.code : "ACCESS_DENIED",
        Date.now(),
      )
      .run()
      .catch(() => {});
    throw error;
  }
}
