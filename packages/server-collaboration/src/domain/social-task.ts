import { z } from "zod";
export const taskSourceSchema = z.object({
    postId: z.string().min(1).max(160),
    sourceVersion: z.number().int().positive(),
    sourceHash: z.string().length(64),
    commentIds: z.array(z.string().max(160)).max(20).default([]),
    aiDraftId: z.string().max(160).nullable().default(null),
});
export type TaskSource = z.infer<typeof taskSourceSchema>;
