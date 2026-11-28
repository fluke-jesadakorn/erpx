import "server-only";
import type {FileStore,SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {PostgresDatabase} from "@erpx/platform-runtime-server/public/runtime/postgres";
import {ATTACHMENT_RESERVATION_MS,attachmentUploadsFrozen,attachmentsV2ReadOnly} from "@erpx/files-server/public/attachments/feature";
import {attachmentPolicy,assertAttachmentMedia} from "@erpx/files-server/public/attachments/policy";
import {attachmentPreviewReadiness,detectedAttachmentMediaType} from "@erpx/files-server/public/attachments/preview";
import {attachmentScannerReadiness} from "@erpx/files-server/public/attachments/scanner";
import {scanAttachmentNow} from "@erpx/files-server/public/attachments/service";
import {legalPortalRepository} from "@erpx/files-server/public/legal-portal-repository";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {hash} from "@erpx/legal-server/adapters/legal/service";
import {createPortalAttachmentApplication} from "@erpx/legal-server/application/portal-attachments";
import type {PortalScope} from "@erpx/legal-server/domain/portal-attachments";
export type {PortalAttachment} from "@erpx/legal-server/domain/portal-attachments";
function application(db:SqlDatabase,files:FileStore){return createPortalAttachmentApplication({repository:legalPortalRepository(db),storage:{put:(key,bytes,mediaType,checksum)=>files.put(key,bytes,{onlyIf:{etagDoesNotMatch:"*"},httpMetadata:{contentType:mediaType,cacheControl:"private, no-store"},customMetadata:{sha256:checksum,state:"quarantined"}}),head:key=>files.head(key),get:key=>files.get(key)},clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},hash,reservationMs:ATTACHMENT_RESERVATION_MS,readOnly:attachmentsV2ReadOnly,uploadsFrozen:attachmentUploadsFrozen,readiness:async()=>{const env={...process.env,ERPX_ATTACHMENTS_V2:"true"};return Promise.all([attachmentScannerReadiness(env),attachmentPreviewReadiness(env)]);},policy:()=>attachmentPolicy("legal_item","legal_portal_submission","action"),assertMedia:(_policy,type,bytes)=>assertAttachmentMedia(attachmentPolicy("legal_item","legal_portal_submission","action"),type,bytes),detectMedia:detectedAttachmentMediaType,scan:id=>scanAttachmentNow(db,files,id),error:(status,code,message)=>new ApiError(status,code,message),errorCode:error=>error instanceof ApiError?error.code:null});}
export function stagePortalAttachment(db:PostgresDatabase,files:FileStore,actorId:string,scope:PortalScope,name:string,declaredMediaType:string,bytes:Uint8Array,commandId:string){return application(db,files).stagePortalAttachment(actorId,scope,name,declaredMediaType,bytes,commandId);}
export function portalAttachmentBytes(db:SqlDatabase,files:FileStore,input:{companyId:string;itemId:string;linkId:string;actorId:string;purposeKey:"legal_document"}){return application(db,files).portalAttachmentBytes(input);}
