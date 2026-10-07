import { z } from "zod";
import type { AttachmentBindingContract, AttachmentManifestItem } from "@/server/modules/files/public/attachments/types";
export const supportEvidenceBindSchema = z.object({ expectedVersion: z.number().int().positive(), attachmentSetIds: z.array(z.string().min(1)).min(1).max(1), reason: z.string().trim().min(3).max(2000) });
export const supportEvidenceRevokeSchema = z.object({ expectedVersion: z.number().int().positive(), attachmentId: z.string().min(1), reason: z.string().trim().min(3).max(2000) });
export type SupportEvidenceBind = z.infer<typeof supportEvidenceBindSchema>;
export type SupportEvidenceRevoke = z.infer<typeof supportEvidenceRevokeSchema>;
export type SupportEvidenceBinding = {
    manifest: AttachmentManifestItem[];
    readonly native: unknown;
};
export const supportEvidenceActive = (status: unknown) => ['open', 'in_progress', 'waiting_customer'].includes(String(status));
export const supportEvidenceBindingContract: AttachmentBindingContract = { targetType: 'support_ticket', bindingType: 'support_action', contexts: ['support_evidence', 'internal_evidence', 'customer_shareable'].map(purposeKey => ({ purposeKey, componentKind: 'action' as const, componentRef: `support-evidence:${purposeKey}` })) };
