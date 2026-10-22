import {createHash} from "node:crypto";
import {createAttachmentAdjudication} from "@erpx/server-files/application/attachments/reconciliation-adjudication";
export type {AttachmentReconciliationSnapshot,AttachmentAdjudicationBlocker,AttachmentAdjudicationItem} from "@erpx/server-files/application/attachments/reconciliation-adjudication";
export const {holdingAttachmentReconciliationReady,buildAttachmentAdjudicationManifest}=createAttachmentAdjudication(value=>createHash("sha256").update(value).digest("hex"));
