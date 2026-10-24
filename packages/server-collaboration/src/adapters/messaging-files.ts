import { z } from 'zod';
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
import { createAttachmentSet, reserveAttachmentFile, completeAttachmentUpload, scanAttachmentNow,withdrawAttachmentLink } from '@erpx/server-files/public/attachments/service';
import { collaborationMessageSource } from '@erpx/server-files/public/collaboration-message-sources';
import {withMessagingUploadRollback} from '@erpx/server-files/public/messaging-upload-rollback';
import { metadata } from '@erpx/server-collaboration/domain/messaging-files';
import {withMessagingPurchasedCommand} from '@erpx/server-collaboration/adapters/messaging-purchases';
import {workPackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import { createMessagingFileApplication,type MessagingFilePorts } from '@erpx/server-collaboration/application/messaging-files';
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
    const mode=await workPackageEnforcementMode();
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

/** Discard only the original uploader's still-unbound evidence through normal
 * Files withdrawal. The source and original bytes remain retained history. */
export async function discardMessagingFile(db:SqlDatabase,actor:WorkContext,id:string,input:{reason:string},key:string){
 const reason=input.reason.trim();if(reason.length<3||reason.length>1000)throw new ApiError(422,'MESSAGE_FILE_DISCARD_REASON_REQUIRED','Explain why this unsent attachment is being discarded.');
 const file=await messagingFileForDiscard(db,actor,id);return withMessagingPurchasedCommand(db,actor,{key,operation:'messaging.file-discard',id:file.room_id,privateFiles:true,alwaysOwned:true,purpose:'settle-existing'},async tx=>{
  const original=await messagingFileForDiscard(tx,actor,id);if(original.room_id!==file.room_id)throw new ApiError(409,'MESSAGING_FILE_SOURCE_CHANGED','The original unsent attachment moved to another conversation.');const fingerprint={id,reason},prior=await replayCommand<{id:string;state:'withdrawn'}>(tx,actor,key,'messaging.file-discard',original.room_id,fingerprint);if(prior)return prior;
  if(!original.attachment_link_id||original.message_id!==null||original.created_by!==actor.userId)throw new ApiError(409,'MESSAGE_FILE_ALREADY_BOUND','Only your own unsent message attachment can be discarded.');
  await withdrawAttachmentLink(tx,actor,original.attachment_link_id,reason,key,{compatibilityAdapter:true});return executeCommand(tx,actor,key,'messaging.file-discard',fingerprint,original.room_id,Number(original.version),{id,state:'withdrawn' as const},[messagingGuard(tx,actor,key,original.room_id)]);
 });
}
async function messagingFileForDiscard(database:SqlDatabase,actor:WorkContext,id:string){const file=await database.prepare('SELECT file.id,file.room_id,file.created_by,file.message_id,file.attachment_link_id,room.version FROM messaging_files file JOIN messaging_rooms room ON room.id=file.room_id WHERE file.id=? AND room.company_id=?').bind(id,actor.companyId).first<{id:string;room_id:string;created_by:string;message_id:string|null;attachment_link_id:string|null;version:number}>();if(!file)throw new ApiError(404,'MESSAGE_FILE_NOT_FOUND','The original unsent attachment is unavailable.');await messagingRoom(database,actor,file.room_id);return file;}
