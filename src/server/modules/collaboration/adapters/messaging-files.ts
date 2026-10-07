import { z } from 'zod';
import type { WorkContext } from '../../identity_access/public/authorization';
import type { FileStore, SqlDatabase, SqlStatement } from '../../runtime/public/runtime/types';
import { ApiError } from '../../runtime/public/http';
import { BusinessRuleError } from '../../shared';
import { messagingRoom, messagingGuard } from '../public/messaging';
import { sha256 } from '../../runtime/public/crypto';
import { executeCommand, replayCommand } from '../../workflow/public/commands';
import { attachmentsV2ReadOnly } from '../../files/public/attachments/feature';
import { detectedAttachmentMediaType } from '../../files/public/attachments/preview';
import { createAttachmentSet, reserveAttachmentFile, completeAttachmentUpload, scanAttachmentNow } from '../../files/public/attachments/service';
import { collaborationMessageSource } from '../../files/public/collaboration-message-sources';
import { metadata } from '../domain/messaging-files';
import { createMessagingFileApplication } from '../application/messaging-files';
async function inward<T>(run: () => Promise<T>) { try {
    return await run();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
export function validMessagingSignature(type: string, data: Uint8Array) { return detectedAttachmentMediaType(data, type) === type; }
export async function uploadMessagingFile(db: SqlDatabase, files: FileStore, actor: WorkContext, roomId: string, input: z.infer<typeof metadata>, request: Request) {
    const app = createMessagingFileApplication<SqlStatement>({ clock: { now: () => Date.now() }, commands: { replay: (...args) => inward(() => replayCommand(db, ...args)), commit: (...args) => inward(() => executeCommand(db, ...args)) }, repository: { bindFile: (...args) => db.prepare('INSERT INTO messaging_files(id,room_id,created_by,original_name,media_type,bytes,object_key,checksum,created_at,attachment_set_id,attachment_link_id,attachment_source_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(...args) }, room: (...args) => inward(() => messagingRoom(db, ...args)), roomGuard: (...args) => messagingGuard(db, ...args), readOnly: attachmentsV2ReadOnly, contentLength: () => Number(request.headers.get('content-length')), openInput: () => request.body?.getReader(), validSignature: validMessagingSignature, sha256, files: { createSet: (...args) => inward(() => createAttachmentSet(db, ...args)), reserveFile: (...args) => inward(() => reserveAttachmentFile(db, ...args)), completeUpload: (a, i, b, o) => inward(() => completeAttachmentUpload(db, files, a, i, b, o)), scan: source => scanAttachmentNow(db, files, source), source: (...args) => collaborationMessageSource(db, ...args) } });
    try {
        return await app.uploadMessagingFile(actor, roomId, input);
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
