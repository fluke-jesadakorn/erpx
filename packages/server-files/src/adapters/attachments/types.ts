import type {SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import type {PreparedAttachmentBinding as Binding} from "@erpx/server-files/domain/attachments/types";
export type PreparedAttachmentBinding=Binding<SqlStatement>;
export {attachmentComponentKinds,attachmentSourceStates,attachmentLinkStates} from "@erpx/server-files/domain/attachments/types";
export type {AttachmentComponentKind,AttachmentSourceState,AttachmentLinkState,AttachmentClassification,AttachmentManifestItem,AttachmentLinkDto,AttachmentSetDto,AttachmentBindingContext,AttachmentBindingContract,AttachmentTargetScope,AttachmentOperation,AttachmentPolicy} from "@erpx/server-files/domain/attachments/types";
