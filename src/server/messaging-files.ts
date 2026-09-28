import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { FileStore, SqlDatabase } from "./runtime/types";
import { messagingGuard, messagingRoom } from "./messaging";
import { executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { sha256 } from "./crypto";
import { attachmentsV2ReadOnly } from "./attachments/feature";
import { detectedAttachmentMediaType } from "./attachments/preview";
import {
  completeAttachmentUpload,
  createAttachmentSet,
  reserveAttachmentFile,
  scanAttachmentNow,
} from "./attachments/service";

const metadata = z.object({
  id: z.string().uuid(),
  name: z.string().trim().min(1).max(255),
  type: z.enum([
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/heic",
    "image/heif",
    "application/pdf",
    "text/csv",
  ]),
});
export function validMessagingSignature(type: string, data: Uint8Array) {
  return detectedAttachmentMediaType(data, type) === type;
}
export async function uploadMessagingFile(
  db: SqlDatabase,
  files: FileStore,
  actor: WorkContext,
  roomId: string,
  input: z.infer<typeof metadata>,
  request: Request,
) {
  input = metadata.parse(input);
  const room = await messagingRoom(db, actor, roomId);
  if(attachmentsV2ReadOnly())throw new ApiError(503,"ATTACHMENTS_READ_ONLY","Attachment intake is disabled during rollback. Existing clean bound evidence remains readable.");
  // Read with a hard bound, including chunked requests without Content-Length.
  if (Number(request.headers.get("content-length")) > 10 * 1024 * 1024)
    throw new ApiError(422, "FILE_SIZE", "Files must be at most 10 MB.");
  const reader = request.body?.getReader();
  if (!reader) throw new ApiError(422, "FILE_REQUIRED", "Select a file.");
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.length;
      if (length > 10 * 1024 * 1024) {
        await reader.cancel();
        throw new ApiError(422, "FILE_SIZE", "Files must be at most 10 MB.");
      }
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  if (!length || !validMessagingSignature(input.type, bytes))
    throw new ApiError(
      422,
      "FILE_TYPE",
      "The file contents do not match its type.",
    );
  const checksum = await sha256(bytes.buffer),
    key = `chat-upload-${input.id}`,
    fingerprint = { ...input, checksum, roomId };
  const prior = await replayCommand<{ id: string }>(
    db,
    actor,
    key,
    "messaging.upload",
    roomId,
    fingerprint,
  );
  if (prior) return prior;
  const set = await createAttachmentSet(
    db,
    actor,
    {
      targetType: "messaging_room",
      targetId: roomId,
      componentKind: "message",
      componentRef: input.id,
      purposeKey: "message_attachment",
      expectedTargetVersion: Number(room.version),
    },
    `${key}:set`,
    { compatibilityAdapter: true },
  );
  const reserved = await reserveAttachmentFile(
    db,
    actor,
    set.id,
    {
      filename: input.name,
      mediaType: input.type,
      bytes: length,
    },
    `${key}:file`,
    { compatibilityAdapter: true },
  );
  const completed = await completeAttachmentUpload(
    db,
    files,
    actor,
    reserved.uploadIntentId,
    bytes.buffer,
    { compatibilityAdapter: true },
  );
  const sourceState = await scanAttachmentNow(db, files, completed.sourceId);
  if (sourceState !== "available")
    throw new ApiError(
      sourceState === "malware_rejected" ? 422 : 503,
      sourceState === "malware_rejected"
        ? "MESSAGE_FILE_REJECTED"
        : "MESSAGE_FILE_SCAN_UNAVAILABLE",
      sourceState === "malware_rejected"
        ? "The message file was rejected by malware scanning."
        : "The message file could not pass malware scanning.",
    );
  const source = await db
    .prepare(
      "SELECT object_key,checksum_sha256 FROM attachments WHERE id=? AND company_id=?",
    )
    .bind(completed.sourceId, actor.companyId)
    .first<{ object_key: string; checksum_sha256: string }>();
  if (!source)
    throw new ApiError(
      503,
      "ATTACHMENT_SOURCE_UNAVAILABLE",
      "The quarantined message file could not be recorded.",
    );
  const result = {
    id: input.id,
    attachmentSetId: set.id,
    attachmentLinkId: completed.attachmentLinkId,
    sourceId: completed.sourceId,
    state: sourceState,
  };
  return executeCommand(
    db,
    actor,
    key,
    "messaging.upload",
    fingerprint,
    roomId,
    Number(room.version),
    result,
    [
      messagingGuard(db, actor, key, roomId),
      db
        .prepare(
          `INSERT INTO messaging_files(
              id,room_id,created_by,original_name,media_type,bytes,object_key,checksum,created_at,
              attachment_set_id,attachment_link_id,attachment_source_id
            ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`,
        )
        .bind(
          input.id,
          roomId,
          actor.userId,
          input.name,
          input.type,
          length,
          source.object_key,
          source.checksum_sha256,
          Date.now(),
          set.id,
          completed.attachmentLinkId,
          completed.sourceId,
        ),
    ],
  );
}
