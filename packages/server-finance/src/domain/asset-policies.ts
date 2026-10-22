import { z } from 'zod';
import { depreciationPolicySchema } from '@erpx/server-finance/domain/finance-instrument-policy';
import { instrumentDecisionSchema, instrumentReversalSchema } from '@erpx/server-finance/domain/instrument-decisions';
export const assetPolicySchema = z.object({
    assetId: z.string().min(1),
    expectedVersion: z.number().int().positive(),
    parentId: z.string().min(1).optional(),
    parentVersion: z.number().int().positive().optional(),
    effectiveDate: z.string().date().optional(),
    adoptLegacy: z.boolean().optional(),
    books: z
        .array(z.object({
        chartId: z.string().min(1),
        policy: depreciationPolicySchema,
    }))
        .min(1)
        .max(20),
    dimensions: z.record(z.string(), z.string()).default({}),
    reason: z.string().trim().min(3).max(2000),
});
export const assetPolicyDecisionSchema = instrumentDecisionSchema;
export const assetPolicyReversalSchema = instrumentReversalSchema;
export type Row = Record<string, string | number | null>;
export type Book = {
    chartId: string;
    costAccount: string;
    accumulatedAccount: string;
    expenseAccount: string;
    ruleId: string | null;
};
export type Result = {
    ordinal: number;
    units: number;
    books: Array<{
        chartId: string;
        amount: string;
        accumulated: string;
        units: number;
    }>;
};
