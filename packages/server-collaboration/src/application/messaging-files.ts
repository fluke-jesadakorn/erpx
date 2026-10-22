import { z } from 'zod';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
import { metadata } from '@erpx/server-collaboration/domain/messaging-files';
export interface MessageFileCommands<Effect> {
    replay<Result>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    commit<Result>(actor: WorkContext, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[]): Promise<Result>;
}
export interface MessagingFilePorts<Effect> {
    clock: Clock;
    owned?<Result>(key:string,run:(unit:MessagingFilePorts<Effect>)=>Promise<Result>):Promise<Result>;
    commands: MessageFileCommands<Effect>;
    repository: {
        bindFile(id: string, room: string, user: string, name: string, type: string, bytes: number, object: string, checksum: string, now: number, set: string, link: string, source: string): Effect;
    };
    room(actor: WorkContext, id: string): Promise<{
        version: number;
    }>;
    roomGuard(actor: WorkContext, key: string, id: string): Effect;
    readOnly(): boolean;
    contentLength(): number;
    openInput(): ReadableStreamDefaultReader<Uint8Array> | undefined;
    validSignature(type: string, bytes: Uint8Array): boolean;
    sha256(bytes: ArrayBuffer): Promise<string>;
    files: {
        createSet(actor: WorkContext, input: {
            targetType: 'messaging_room';
            targetId: string;
            componentKind: 'message';
            componentRef: string;
            purposeKey: 'message_attachment';
            expectedTargetVersion: number;
        }, key: string, options: {
            compatibilityAdapter: true;
        }): Promise<{
            id: string;
        }>;
        reserveFile(actor: WorkContext, set: string, input: {
            filename: string;
            mediaType: string;
            bytes: number;
        }, key: string, options: {
            compatibilityAdapter: true;
        }): Promise<{
            uploadIntentId: string;
        }>;
        completeUpload(actor: WorkContext, intent: string, bytes: ArrayBuffer, options: {
            compatibilityAdapter: true;
        }): Promise<{
            sourceId: string;
            attachmentLinkId: string;
        }>;
        scan(source: string): Promise<string>;
        source(source: string, company: string): Promise<{
            object_key: string;
            checksum_sha256: string;
        } | null>;
    };
}
export function createMessagingFileApplication<Effect>(ports: MessagingFilePorts<Effect>) {
    async function uploadMessagingFile(actor: WorkContext, roomId: string, input: z.infer<typeof metadata>) {
        input = metadata.parse(input);
        const originalRoom = await ports.room(actor, roomId);
        if (ports.readOnly())
            throw new BusinessRuleError(503, "ATTACHMENTS_READ_ONLY", "Attachment intake is disabled during rollback. Existing clean bound evidence remains readable.");
        // Read with a hard bound, including chunked requests without Content-Length.
        if (ports.contentLength() > 10 * 1024 * 1024)
            throw new BusinessRuleError(422, "FILE_SIZE", "Files must be at most 10 MB.");
        const reader = ports.openInput();
        if (!reader)
            throw new BusinessRuleError(422, "FILE_REQUIRED", "Select a file.");
        const chunks: Uint8Array[] = [];
        let length = 0;
        try {
            for (;;) {
                const { done, value } = await reader.read();
                if (done)
                    break;
                length += value.length;
                if (length > 10 * 1024 * 1024) {
                    await reader.cancel();
                    throw new BusinessRuleError(422, "FILE_SIZE", "Files must be at most 10 MB.");
                }
                chunks.push(value);
            }
        }
        finally {
            reader.releaseLock();
        }
        const bytes = new Uint8Array(length);
        let offset = 0;
        for (const chunk of chunks) {
            bytes.set(chunk, offset);
            offset += chunk.length;
        }
        if (!length || !ports.validSignature(input.type, bytes))
            throw new BusinessRuleError(422, "FILE_TYPE", "The file contents do not match its type.");
        const checksum = await ports.sha256(bytes.buffer), key = `chat-upload-${input.id}`, fingerprint = { ...input, checksum, roomId };
        const submit=async(unit:MessagingFilePorts<Effect>)=>{
        const room=unit===ports?originalRoom:await unit.room(actor,roomId);
        const prior = await unit.commands.replay<{
            id: string;
            attachmentSetId: string;
            attachmentLinkId: string;
            sourceId: string;
            state: string;
        }>(actor, key, "messaging.upload", roomId, fingerprint);
        if (prior)
            return prior;
        const set = await unit.files.createSet(actor, {
            targetType: "messaging_room",
            targetId: roomId,
            componentKind: "message",
            componentRef: input.id,
            purposeKey: "message_attachment",
            expectedTargetVersion: Number(room.version),
        }, `${key}:set`, { compatibilityAdapter: true });
        const reserved = await unit.files.reserveFile(actor, set.id, {
            filename: input.name,
            mediaType: input.type,
            bytes: length,
        }, `${key}:file`, { compatibilityAdapter: true });
        const completed = await unit.files.completeUpload(actor, reserved.uploadIntentId, bytes.buffer, { compatibilityAdapter: true });
        const sourceState = await unit.files.scan(completed.sourceId);
        if (sourceState !== "available")
            throw new BusinessRuleError(sourceState === "malware_rejected" ? 422 : 503, sourceState === "malware_rejected"
                ? "MESSAGE_FILE_REJECTED"
                : "MESSAGE_FILE_SCAN_UNAVAILABLE", sourceState === "malware_rejected"
                ? "The message file was rejected by malware scanning."
                : "The message file could not pass malware scanning.");
        const source = await unit.files.source(completed.sourceId, actor.companyId);
        if (!source)
            throw new BusinessRuleError(503, "ATTACHMENT_SOURCE_UNAVAILABLE", "The quarantined message file could not be recorded.");
        const result = {
            id: input.id,
            attachmentSetId: set.id,
            attachmentLinkId: completed.attachmentLinkId,
            sourceId: completed.sourceId,
            state: sourceState,
        };
        return unit.commands.commit(actor, key, "messaging.upload", fingerprint, roomId, Number(room.version), result, [
            unit.roomGuard(actor, key, roomId),
            unit.repository.bindFile(input.id, roomId, actor.userId, input.name, input.type, length, source.object_key, source.checksum_sha256, unit.clock.now(), set.id, completed.attachmentLinkId, completed.sourceId),
        ]);
        };
        return ports.owned?ports.owned(key,submit):submit(ports);
    }
    return { uploadMessagingFile };
}
