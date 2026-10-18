export { customerPortalWorkContext } from "../domain/portal";
export type { CustomerPortalSessionActor, CustomerPortalBindingScope, CustomerSharedFile } from "../domain/portal";
export { customerPortalBindingScope, customerPortalAttachmentSession, customerPortalAttachmentContext, customerPortalAttachmentTarget, customerPortalSharedFiles, customerPortalSharedPreview } from "../adapters/portal-attachments";
export {customerPortalSharedPreviewRequest} from "../adapters/shared-preview-request";
export {customerPortalUploadRequest,customerPortalEvidenceRequest,customerPortalAttachmentCommandRequest,customerPortalAttachmentMetadataRequest} from '../adapters/attachment-request';
