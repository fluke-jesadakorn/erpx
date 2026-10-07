import {z} from 'zod';export const leaveEvidencePurposes = [
  "medical_certificate",
  "leave_supporting_photo",
] as const;
export type LeaveEvidencePurpose = (typeof leaveEvidencePurposes)[number];
export type LeaveEvidencePolicy = {
  required: boolean;
  minimumUnits: number | null;
  purposeKey: LeaveEvidencePurpose | null;
  minimumCount: number;
  dueStage: "before_submit" | "before_final_approval";
  graceDays: number;
  privacy: "requester_approvers_hr";
  note: string;
};
export const leaveEvidencePolicySchema = z.object({
  required: z.boolean(),
  minimumUnits: z.number().nullable(),
  purposeKey: z.enum(leaveEvidencePurposes).nullable(),
  minimumCount: z.number().int().min(0).max(100),
  dueStage: z.enum(["before_submit", "before_final_approval"]),
  graceDays: z.number().int().min(0).max(365),
  privacy: z.literal("requester_approvers_hr"),
  note: z.string(),
});
export type LeaveAttachmentTarget = {
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  created_by: string;
  state: string;
  version: number;
};
export function parseLeaveEvidencePolicy(value: unknown): LeaveEvidencePolicy | null {
  if (typeof value === "string") {
    try {
      value = JSON.parse(value);
    } catch {
      return null;
    }
  }
  const parsed = leaveEvidencePolicySchema.safeParse(value);
  return parsed.success ? parsed.data : null;
}
