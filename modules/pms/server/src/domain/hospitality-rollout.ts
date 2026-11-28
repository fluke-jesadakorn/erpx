import { z } from "zod";
export const changeSchema = z.object({
    moduleId: z.string().min(1).max(80),
    state: z.enum(['off', 'pilot', 'available']),
    expectedVersion: z.number().int().min(0),
    reason: z.string().trim().min(3).max(1000),
});
