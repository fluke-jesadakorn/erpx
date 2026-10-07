import {createAttachmentReadiness} from "../../application/attachments/readiness";
import type {AttachmentEnvironment} from "../../domain/attachments/feature";
import {attachmentScannerReadiness} from "../../public/attachments/scanner";
import {attachmentPreviewReadiness} from "../../public/attachments/preview";
const readiness=createAttachmentReadiness({scanner:environment=>attachmentScannerReadiness(environment as NodeJS.ProcessEnv),preview:environment=>attachmentPreviewReadiness(environment as NodeJS.ProcessEnv)});
export function attachmentRuntimeReadiness(environment:AttachmentEnvironment=process.env){return readiness.attachmentRuntimeReadiness(environment);}
