import { z } from 'zod';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { FileStore, SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import {messagingRoom} from '@erpx/server-collaboration/public/messaging-read';
import {messagingGuard} from '@erpx/server-collaboration/public/messaging';
import { sha256 } from '@erpx/server-runtime/public/crypto';
import { executeCommand, replayCommand } from '@erpx/server-workflow/public/commands';
import { attachmentsV2ReadOnly } from '@erpx/server-files/public/attachments/feature';
import { detectedAttachmentMediaType } from '@erpx/server-files/public/attachments/preview';
import { createAttachmentSet, reserveAttachmentFile, completeAttachmentUpload, scanAttachmentNow } from '@erpx/server-files/public/attachments/service';
import { collaborationMessageSource } from '@erpx/server-files/public/collaboration-message-sources';
import {withMessagingUploadRollback} from '@erpx/server-files/public/messaging-upload-rollback';
import { metadata } from '@erpx/server-collaboration/domain/messaging-files';
import {withMessagingPurchasedCommand} from '@erpx/server-collaboration/adapters/messaging-purchases';
import {appsPackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import { createMessagingFileApplication,type MessagingFilePorts } from '@erpx/server-collaboration/application/messaging-files';
import { nativeDiscardMessagingFile } from '@erpx/server-collaboration/public/native-messaging-page';
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
    const mode=await appsPackageEnforcementMode();
    const submit=async(uploadFiles:FileStore,rememberIntent:(id:string)=>void)=>{
    const portsFor=(db:SqlDatabase,owned=true):MessagingFilePorts<SqlStatement>=>({ clock: { now: () => Date.now() }, ...(owned&&mode!=='disabled'?{owned:<Result>(key:string,run:(unit:MessagingFilePorts<SqlStatement>)=>Promise<Result>)=>withMessagingPurchasedCommand(db,actor,{key,operation:'messaging.upload',id:roomId,privateFiles:true,purpose:'new-work'},tx=>run(portsFor(tx,false)))}:{}), commands: { replay: (...args) => inward(() => replayCommand(db, ...args)), commit: (...args) => inward(() => executeCommand(db, ...args)) }, repository: { bindFile: (...args) => db.prepare('INSERT INTO messaging_files(id,room_id,created_by,original_name,media_type,bytes,object_key,checksum,created_at,attachment_set_id,attachment_link_id,attachment_source_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(...args) }, room: (...args) => inward(() => messagingRoom(db, ...args)), roomGuard: (...args) => messagingGuard(db, ...args), readOnly: attachmentsV2ReadOnly, contentLength: () => Number(request.headers.get('content-length')), openInput: () => request.body?.getReader(), validSignature: validMessagingSignature, sha256, files: { createSet: (...args) => inward(() => createAttachmentSet(db, ...args)), reserveFile: async (...args) => {const reservation=await inward(() => reserveAttachmentFile(db, ...args));rememberIntent(reservation.uploadIntentId);return reservation;}, completeUpload: (a, i, b, o) => inward(() => completeAttachmentUpload(db, uploadFiles, a, i, b, o)), scan: source => scanAttachmentNow(db, uploadFiles, source), source: (...args) => collaborationMessageSource(db, ...args) } });
    const app=createMessagingFileApplication(portsFor(db));
    return app.uploadMessagingFile(actor,roomId,input);
    };
    try {
        return mode==='disabled'?await submit(files,()=>undefined):await withMessagingUploadRollback(db,files,actor,({files:tracked,rememberIntent})=>submit(tracked,rememberIntent));
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}

/** Discard only the original uploader's still-unbound evidence through the
 * native Messaging owner command. Replay, the conversation guard, the Files
 * withdrawal and the purchase settlement commit in one serializable unit; the
 * source and original bytes remain retained history. */
export async function discardMessagingFile(_db:SqlDatabase,actor:WorkContext,id:string,input:{reason:string},key:string){
 const reason=input.reason.trim();if(reason.length<3||reason.length>1000)throw new ApiError(422,'MESSAGE_FILE_DISCARD_REASON_REQUIRED','Explain why this unsent attachment is being discarded.');
 const value=await nativeDiscardMessagingFile(await headers(),actor.holdingSlug,id,reason,key);
 if(!value||typeof value!=='object'||Array.isArray(value)||Object.keys(value).length!==2||(value as Record<string,unknown>).id!==id||(value as Record<string,unknown>).state!=='withdrawn')
  throw new ApiError(503,'NATIVE_COLLABORATION_RESPONSE_INVALID','The native Messaging receipt was invalid.');
 return value as {id:string;state:'withdrawn'};
}
