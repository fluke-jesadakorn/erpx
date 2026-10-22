import "server-only";
import type {FileStore,SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import {ATTACHMENT_RESERVATION_MS,attachmentUploadsFrozen,attachmentsV2ReadOnly} from "@erpx/server-files/public/attachments/feature";
import {attachmentPolicy,assertAttachmentMedia} from "@erpx/server-files/public/attachments/policy";
import {attachmentPreviewReadiness,detectedAttachmentMediaType} from "@erpx/server-files/public/attachments/preview";
import {attachmentScannerReadiness} from "@erpx/server-files/public/attachments/scanner";
import {scanAttachmentNow} from "@erpx/server-files/public/attachments/service";
import {legalPortalRepository} from "@erpx/server-files/public/legal-portal-repository";
import {ApiError} from "@erpx/server-runtime/public/http";
import {hash} from "@erpx/server-legal/adapters/legal/service";
import {createPortalAttachmentApplication} from "@erpx/server-legal/application/portal-attachments";
import type {PortalScope} from "@erpx/server-legal/domain/portal-attachments";
export type {PortalAttachment} from "@erpx/server-legal/domain/portal-attachments";
function application(db:SqlDatabase,files:FileStore){return createPortalAttachmentApplication({repository:legalPortalRepository(db),storage:{put:(key,bytes,mediaType,checksum)=>files.put(key,bytes,{onlyIf:{etagDoesNotMatch:"*"},httpMetadata:{contentType:mediaType,cacheControl:"private, no-store"},customMetadata:{sha256:checksum,state:"quarantined"}}),head:key=>files.head(key),get:key=>files.get(key)},clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},hash,reservationMs:ATTACHMENT_RESERVATION_MS,readOnly:attachmentsV2ReadOnly,uploadsFrozen:attachmentUploadsFrozen,readiness:async()=>{const env={...process.env,ERPX_ATTACHMENTS_V2:"true"};return Promise.all([attachmentScannerReadiness(env),attachmentPreviewReadiness(env)]);},policy:()=>attachmentPolicy("legal_item","legal_portal_submission","action"),assertMedia:(_policy,type,bytes)=>assertAttachmentMedia(attachmentPolicy("legal_item","legal_portal_submission","action"),type,bytes),detectMedia:detectedAttachmentMediaType,scan:id=>scanAttachmentNow(db,files,id),error:(status,code,message)=>new ApiError(status,code,message),errorCode:error=>error instanceof ApiError?error.code:null});}
export function stagePortalAttachment(db:PostgresDatabase,files:FileStore,actorId:string,scope:PortalScope,name:string,declaredMediaType:string,bytes:Uint8Array,commandId:string){return application(db,files).stagePortalAttachment(actorId,scope,name,declaredMediaType,bytes,commandId);}
export function portalAttachmentBytes(db:SqlDatabase,files:FileStore,input:{companyId:string;itemId:string;linkId:string;actorId:string;purposeKey:"legal_document"}){return application(db,files).portalAttachmentBytes(input);}
