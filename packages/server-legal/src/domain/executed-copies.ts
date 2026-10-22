import { z } from "zod";
export const executedCopyChecks = z.object({ contentMatches: z.boolean(), pagesComplete: z.boolean(), partiesVerified: z.boolean(), signaturesComplete: z.boolean(), authorityVerified: z.boolean() }).strict();
export const executedCopyReview = z.object({ action: z.literal("executed.review"), id: z.string().min(1), expectedVersion: z.number().int().positive(), executedCopyId: z.string().min(1), decision: z.enum(["confirm", "changes_required"]), checks: executedCopyChecks, reason: z.string().trim().min(10).max(4000) }).strict();
export type ExecutedCopyReview = z.infer<typeof executedCopyReview>;
