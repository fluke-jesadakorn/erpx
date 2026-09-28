import type { SqlDatabase, FileStore } from "./runtime/types";
import type { WorkContext } from "./authorization";
import { ApiError } from "./http";
import { assertion, executeCommand, replayCommand } from "./commands";
import { expenseAiSource, expenseAiAllowed } from "./expense-ai-source";
import { expenseAuthoritySnapshot } from "./expense-routing";
import { sha256 } from "./crypto";
import {
  attachmentUploadsFrozen,
  attachmentsV2ReadOnly,
} from "./attachments/feature";
export async function saveDocumentPage(
  db: SqlDatabase,
  files: FileStore,
  actor: WorkContext,
  attachmentId: string,
  input: {
    page: number;
    pageCount: number;
    originalChecksum: string;
    bytes: ArrayBuffer;
  },
  key: string,
) {
  if (attachmentsV2ReadOnly())
    throw new ApiError(
      503,
      "ATTACHMENTS_READ_ONLY",
      "Attachment intake is disabled during rollback. Existing clean bound evidence remains readable.",
    );
  if (attachmentUploadsFrozen())
    throw new ApiError(
      503,
      "ATTACHMENT_UPLOAD_FREEZE",
      "File intake is briefly paused for storage reconciliation.",
    );
  if (
    input.page < 1 ||
    input.page > input.pageCount ||
    input.pageCount > 20 ||
    !Number.isInteger(input.page) ||
    !Number.isInteger(input.pageCount) ||
    input.bytes.byteLength > 2 * 1024 * 1024
  )
    throw new ApiError(
      422,
      "DOCUMENT_PAGE_LIMIT",
      "Use up to 20 pages, each at most 2 MB.",
    );
  const bytes = new Uint8Array(input.bytes);
  if (bytes[0] !== 0xff || bytes[1] !== 0xd8)
    throw new ApiError(422, "PAGE_TYPE", "Rendered pages must be JPEG images.");
  const attachment = await db
    .prepare(
      `SELECT * FROM attachments
       WHERE id=? AND company_id=? AND revoked_at IS NULL
         AND source_state='available'
         AND owner_type IN ('expense_claim','expense_payment')`,
    )
    .bind(attachmentId, actor.companyId)
    .first<{
      owner_type: string;
      owner_id: string;
      checksum_sha256: string;
      media_type: string;
    }>();
  if (!attachment || attachment.checksum_sha256 !== input.originalChecksum)
    throw new ApiError(
      404,
      "DOCUMENT_NOT_FOUND",
      "The original document is unavailable or changed.",
    );
  if (
    !["application/pdf", "image/jpeg", "image/png", "image/webp"].includes(
      attachment.media_type,
    ) ||
    (attachment.media_type !== "application/pdf" && input.pageCount !== 1)
  )
    throw new ApiError(
      422,
      "DOCUMENT_TYPE",
      "This document cannot be rendered for image preparation.",
    );
  const paymentId =
    attachment.owner_type === "expense_payment" ? attachment.owner_id : null;
  const payment = paymentId
    ? await db
        .prepare(
          "SELECT expense_id FROM expense_payment_drafts WHERE id=? AND company_id=?",
        )
        .bind(paymentId, actor.companyId)
        .first<{ expense_id: string }>()
    : null;
  const expenseId = paymentId ? payment?.expense_id : attachment.owner_id;
  if (!expenseId)
    throw new ApiError(
      404,
      "DOCUMENT_NOT_FOUND",
      "The document is not in this company.",
    );
  const authority = await expenseAuthoritySnapshot(db, actor.companyId),
    source = await expenseAiSource(db, actor.companyId, expenseId, paymentId);
  if (!(await expenseAiAllowed(db, actor, source.claim, paymentId)))
    throw new ApiError(
      403,
      "DOCUMENT_PREPARATION_FORBIDDEN",
      "Only the person preparing or reviewing the current stage can prepare document pages.",
    );
  const checksum = await sha256(input.bytes),
    safeInput = {
      attachmentId,
      page: input.page,
      pageCount: input.pageCount,
      originalChecksum: input.originalChecksum,
      checksum,
    },
    operation = "expense.document.render";
  const prior = await replayCommand(
    db,
    actor,
    key,
    operation,
    expenseId,
    safeInput,
  );
  if (prior) return prior;
  const existing = await db
    .prepare(
      "SELECT id,checksum_sha256 FROM attachment_derivatives WHERE attachment_id=? AND page_number=? AND original_checksum=?",
    )
    .bind(attachmentId, input.page, input.originalChecksum)
    .first<{ id: string; checksum_sha256: string }>();
  if (existing) return { id: existing.id, page: input.page };
  const id = crypto.randomUUID(),
    objectKey = `${actor.holdingId}/document-pages/${id}`,
    now = Date.now();
  await files.put(objectKey, input.bytes, {
    httpMetadata: { contentType: "image/jpeg" },
    customMetadata: { sha256: checksum },
  });
  try {
    return await executeCommand(
      db,
      actor,
      key,
      operation,
      safeInput,
      expenseId,
      source.claim.version,
      { id, page: input.page },
      [
        authority.guard(key),
        ...source.guards(key),
        assertion(
          db,
          key,
          "page-count",
          "NOT EXISTS(SELECT 1 FROM attachment_documents WHERE attachment_id=? AND (original_checksum!=? OR page_count!=?))",
          [attachmentId, input.originalChecksum, input.pageCount],
        ),
        assertion(
          db,
          key,
          "page-storage",
          `COALESCE((SELECT SUM(bytes) FROM attachment_storage_ledger
             WHERE company_id=? AND state IN ('reserved','stored')),0)
           +COALESCE((SELECT SUM(declared_bytes) FROM upload_intents
             WHERE company_id=? AND status='pending' AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint),0)
           +?<=COALESCE((SELECT storage_bytes FROM beta_policy WHERE company_id=?),9007199254740991)`,
          [
            actor.companyId,
            actor.companyId,
            input.bytes.byteLength,
            actor.companyId,
          ],
        ),
        db
          .prepare(
            "INSERT INTO attachment_documents(attachment_id,company_id,original_checksum,page_count,created_at) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING",
          )
          .bind(
            attachmentId,
            actor.companyId,
            input.originalChecksum,
            input.pageCount,
            now,
          ),
        db
          .prepare(
            "INSERT INTO attachment_derivatives(id,attachment_id,company_id,page_number,object_key,checksum_sha256,original_checksum,bytes,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            attachmentId,
            actor.companyId,
            input.page,
            objectKey,
            checksum,
            input.originalChecksum,
            input.bytes.byteLength,
            actor.userId,
            now,
          ),
        db
          .prepare(
            "INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES(?,?,?,'derivative',?,?,'stored',?)",
          )
          .bind(
            `attachment-storage-derivative-${id}`,
            actor.companyId,
            attachmentId,
            objectKey,
            input.bytes.byteLength,
            now,
          ),
      ],
    );
  } catch (error) {
    await files.delete(objectKey);
    throw error;
  }
}
