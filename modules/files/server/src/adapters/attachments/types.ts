import type {SqlStatement} from "@erpx/platform-runtime-server/public/runtime/types";
import type {PreparedAttachmentBinding as Binding} from "@erpx/files-server/domain/attachments/types";
export type PreparedAttachmentBinding=Binding<SqlStatement>;
export {attachmentComponentKinds,attachmentSourceStates,attachmentLinkStates} from "@erpx/files-server/domain/attachments/types";
export type {AttachmentComponentKind,AttachmentSourceState,AttachmentLinkState,AttachmentClassification,AttachmentManifestItem,AttachmentLinkDto,AttachmentSetDto,AttachmentBindingContext,AttachmentBindingContract,AttachmentTargetScope,AttachmentOperation,AttachmentPolicy} from "@erpx/files-server/domain/attachments/types";
