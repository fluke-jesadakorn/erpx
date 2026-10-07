import { z } from "zod";
export const levelSchema = z.object({
    code: z
        .string()
        .trim()
        .min(1)
        .max(30)
        .regex(/^[A-Za-z0-9_-]+$/)
        .transform((s) => s.toUpperCase()),
    name: z.string().trim().min(1).max(100),
    rank: z.number().int().min(-10000).max(10000),
    levelNumber: z.number().int().min(1).max(6).optional(),
    active: z.boolean().default(true),
    expectedVersion: z.number().int().min(0).default(0),
});
export const levelAssignmentSchema = z.object({
    levelId: z.string().min(1),
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(1000),
});
export const headSchema = z.object({
    personId: z.string().min(1),
    expectedVersion: z.number().int().min(0),
    reason: z.string().trim().min(3).max(1000),
});
