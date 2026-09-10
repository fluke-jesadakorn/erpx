import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { FileStore, SqlDatabase } from "./runtime/types";
import { messagingGuard, messagingRoom } from "./messaging";
import { executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { sha256 } from "./crypto";

const metadata = z.object({
  id: z.string().uuid(),
  name: z.string().trim().min(1).max(255),
  type: z.enum([
    "image/jpeg",
    "image/png",
    "image/webp",
    "application/pdf",
    "text/csv",
  ]),
});
export function validMessagingSignature(type: string, data: Uint8Array) {
  const start = new TextDecoder().decode(data.slice(0, 12));
  if (type === "application/pdf") return start.startsWith("%PDF-");
  if (type === "image/png")
    return [137, 80, 78, 71, 13, 10, 26, 10].every((v, i) => data[i] === v);
  if (type === "image/jpeg")
    return data[0] === 255 && data[1] === 216 && data[2] === 255;
  if (type === "image/webp")
    return start.startsWith("RIFF") && start.slice(8, 12) === "WEBP";
  return type === "text/csv" && !data.includes(0);
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
  await messagingRoom(db, actor, roomId);
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
  const objectKey = `${actor.holdingId}/messaging/${actor.companyId}/${actor.userId}/${input.id}`;
  const stored = await files.put(objectKey, bytes, {
    onlyIf: { etagDoesNotMatch: "*" },
    httpMetadata: { contentType: input.type },
    customMetadata: { sha256: checksum },
  });
  if (
    !stored &&
    (await files.head(objectKey))?.customMetadata.sha256 !== checksum
  )
    throw new ApiError(
      409,
      "FILE_CONFLICT",
      "Retry with the original file or select a new file.",
    );
  return executeCommand(
    db,
    actor,
    key,
    "messaging.upload",
    fingerprint,
    roomId,
    1,
    { id: input.id },
    [
      messagingGuard(db, actor, key, roomId),
      db
        .prepare(
          "INSERT INTO messaging_files(id,room_id,created_by,original_name,media_type,bytes,object_key,checksum,created_at) VALUES(?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          input.id,
          roomId,
          actor.userId,
          input.name,
          input.type,
          length,
          objectKey,
          checksum,
          Date.now(),
        ),
    ],
  );
}
