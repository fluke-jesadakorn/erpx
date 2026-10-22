import { z } from "zod";
export type LearningCertificate = { id: string; handoff_id: string; company_id: string; person_id: string; assignment_id: string; source_assignment_version: number; eligibility_code: string; valid_from: string; expires_on: string | null; evidence_ref: string };
export type LearningBenefitInput = { personId: string; requiredCode: string; effectiveDate: string; electionId: string; key: string };
export type LearningBenefitElection = { id: string; person_id: string; learning_certificate_id?: string | null; effective_from: string; plan_id: string };
export const certificateRevocationSchema = z.object({ expectedAssignmentVersion: z.number().int().positive(), reason: z.string().trim().min(8).max(2000) }).strict();
export type LearningRevocationInput = z.infer<typeof certificateRevocationSchema>;
