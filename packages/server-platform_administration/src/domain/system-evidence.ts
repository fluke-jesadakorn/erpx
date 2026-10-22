import { z } from "zod";
export const systemEvidenceInput = z.object({ expectedVersion: z.number().int().positive(), attachmentSetIds: z.array(z.string().min(1)).min(1).max(20), reason: z.string().trim().min(3).max(2000) }).strict();
export const systemEvidenceOwners = { iam_review: { purpose: "sanitized_review_evidence", permission: "iam.manage" }, iam_migration: { purpose: "migration_evidence", permission: "iam.manage" }, people_provision_operation: { purpose: "sanitized_provisioning_evidence", permission: "iam.directory.apply" } } as const;
export type SystemEvidenceTarget = keyof typeof systemEvidenceOwners;
export type SystemEvidenceInput = z.infer<typeof systemEvidenceInput>;
