import {createAttachmentReadiness} from "@erpx/server-files/application/attachments/readiness";
import type {AttachmentEnvironment} from "@erpx/server-files/domain/attachments/feature";
import {attachmentScannerReadiness} from "@erpx/server-files/public/attachments/scanner";
import {attachmentPreviewReadiness} from "@erpx/server-files/public/attachments/preview";
const readiness=createAttachmentReadiness({scanner:environment=>attachmentScannerReadiness(environment as NodeJS.ProcessEnv),preview:environment=>attachmentPreviewReadiness(environment as NodeJS.ProcessEnv)});
export function attachmentRuntimeReadiness(environment:AttachmentEnvironment=process.env){return readiness.attachmentRuntimeReadiness(environment);}
