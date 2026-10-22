import "server-only";
import {ApiError} from "@erpx/server-runtime/public/http";
import {createAttachmentFeatures,type AttachmentEnvironment} from "@erpx/server-files/domain/attachments/feature";
export {ATTACHMENT_MAX_BYTES,ATTACHMENT_RESERVATION_MS,ATTACHMENT_DRAFT_MS} from "@erpx/server-files/domain/attachments/feature";
const features=createAttachmentFeatures((status,code,message)=>new ApiError(status,code,message));
export function attachmentsV2Enabled(environment:AttachmentEnvironment=process.env){return features.attachmentsV2Enabled(environment);}
export function attachmentsV2ReadOnly(environment:AttachmentEnvironment=process.env){return features.attachmentsV2ReadOnly(environment);}
export function attachmentsV2Readable(environment:AttachmentEnvironment=process.env){return features.attachmentsV2Readable(environment);}
export function attachmentUploadsFrozen(environment:AttachmentEnvironment=process.env){return features.attachmentUploadsFrozen(environment);}
export function requireAttachmentsV2(environment:AttachmentEnvironment=process.env){return features.requireAttachmentsV2(environment);}
export function requireAttachmentsV2Read(environment:AttachmentEnvironment=process.env){return features.requireAttachmentsV2Read(environment);}
