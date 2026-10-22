import { z } from "zod";
export const recognitionRunSchema = z.object({
    billingRunId: z.string().min(1),
    itemKey: z.string().min(1),
    periodId: z.string().min(1),
    entryDate: z.string().date(),
    reason: z.string().trim().min(3).max(2000),
});
export const assetPolicyRunSchema = z.object({
    expectedVersion: z.number().int().positive(),
    periodId: z.string().min(1),
    entryDate: z.string().date(),
    depreciationMonth: z
        .string()
        .regex(/^\d{4}-(0[1-9]|1[0-2])$/)
        .optional(),
    units: z.number().int().nonnegative().max(1000000000).default(0),
    usageEvidence: z.string().trim().min(3).max(2000).optional(),
    reason: z.string().trim().min(3).max(2000),
});
export const instrumentRunSchema = z.object({
    expectedVersion: z.number().int().positive(),
    kind: z.enum(["payment", "payoff"]).default("payment"),
    periodId: z.string().min(1),
    entryDate: z.string().date(),
    bankTransactionId: z.string().min(1),
    bankVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(2000),
});
