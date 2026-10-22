export type AttachmentEnvironment=Readonly<Record<string,string|undefined>>;
export const ATTACHMENT_MAX_BYTES = 10 * 1024 * 1024;
export const ATTACHMENT_RESERVATION_MS = 15 * 60_000;
export const ATTACHMENT_DRAFT_MS = 7 * 24 * 60 * 60_000;

export function createAttachmentFeatures(error:(status:number,code:string,message:string)=>Error){
function attachmentsV2Enabled(environment: AttachmentEnvironment) {
  return environment.ERPX_ATTACHMENTS_V2 === "true";
}

/**
 * Emergency rollback keeps immutable, clean, bound evidence readable while
 * every command that could create, link, bind or publish a file remains off.
 */
function attachmentsV2ReadOnly(environment: AttachmentEnvironment) {
  return !attachmentsV2Enabled(environment) && environment.ERPX_ATTACHMENTS_READ_ONLY === "true";
}

function attachmentsV2Readable(environment: AttachmentEnvironment) {
  return attachmentsV2Enabled(environment) || attachmentsV2ReadOnly(environment);
}

function attachmentUploadsFrozen(environment: AttachmentEnvironment) {
  return environment.ERPX_ATTACHMENT_UPLOAD_FREEZE === "true";
}

function requireAttachmentsV2(environment: AttachmentEnvironment) {
  if (!attachmentsV2Enabled(environment))
    throw error(404, "ATTACHMENTS_DISABLED", "The attachment platform is not enabled for this release.");
}

function requireAttachmentsV2Read(environment: AttachmentEnvironment) {
  if (!attachmentsV2Readable(environment))
    throw error(404, "ATTACHMENTS_DISABLED", "The attachment platform is not enabled for this release.");
}

return {attachmentsV2Enabled,attachmentsV2ReadOnly,attachmentsV2Readable,attachmentUploadsFrozen,requireAttachmentsV2,requireAttachmentsV2Read};}
