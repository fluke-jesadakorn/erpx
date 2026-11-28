import { z } from 'zod';
export const instrumentDecisionSchema = z.object({
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(2000),
});
export const instrumentReversalSchema = instrumentDecisionSchema.extend({
    periodId: z.string().min(1),
    entryDate: z.string().date(),
});
