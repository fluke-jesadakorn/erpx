import {createAttachmentReadiness} from "@erpx/files-server/application/attachments/readiness";
import type {AttachmentEnvironment} from "@erpx/files-server/domain/attachments/feature";
import {attachmentScannerReadiness} from "@erpx/files-server/public/attachments/scanner";
import {attachmentPreviewReadiness} from "@erpx/files-server/public/attachments/preview";
const readiness=createAttachmentReadiness({scanner:environment=>attachmentScannerReadiness(environment as NodeJS.ProcessEnv),preview:environment=>attachmentPreviewReadiness(environment as NodeJS.ProcessEnv)});
export function attachmentRuntimeReadiness(environment:AttachmentEnvironment=process.env){return readiness.attachmentRuntimeReadiness(environment);}
