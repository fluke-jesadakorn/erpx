import "server-only";

import { ApiError } from "../http";

export function attachmentsV2Enabled(environment: NodeJS.ProcessEnv = process.env) {
  return environment.ERPX_ATTACHMENTS_V2 === "true";
}

/**
 * Emergency rollback keeps immutable, clean, bound evidence readable while
 * every command that could create, link, bind or publish a file remains off.
 */
export function attachmentsV2ReadOnly(environment: NodeJS.ProcessEnv = process.env) {
  return !attachmentsV2Enabled(environment) && environment.ERPX_ATTACHMENTS_READ_ONLY === "true";
}

export function attachmentsV2Readable(environment: NodeJS.ProcessEnv = process.env) {
  return attachmentsV2Enabled(environment) || attachmentsV2ReadOnly(environment);
}

export function attachmentUploadsFrozen(environment: NodeJS.ProcessEnv = process.env) {
  return environment.ERPX_ATTACHMENT_UPLOAD_FREEZE === "true";
}

export function requireAttachmentsV2(environment: NodeJS.ProcessEnv = process.env) {
  if (!attachmentsV2Enabled(environment))
    throw new ApiError(404, "ATTACHMENTS_DISABLED", "The attachment platform is not enabled for this release.");
}

export function requireAttachmentsV2Read(environment: NodeJS.ProcessEnv = process.env) {
  if (!attachmentsV2Readable(environment))
    throw new ApiError(404, "ATTACHMENTS_DISABLED", "The attachment platform is not enabled for this release.");
}

export const ATTACHMENT_MAX_BYTES = 10 * 1024 * 1024;
export const ATTACHMENT_RESERVATION_MS = 15 * 60_000;
export const ATTACHMENT_DRAFT_MS = 7 * 24 * 60 * 60_000;
