import { z } from 'zod';
export const accountingProfileSchema = z.object({
    statutoryChartId: z.string().min(1).max(128),
    standard: z.literal("TFRS for NPAEs"),
    fiscalStartMonth: z.number().int().min(1).max(12),
    effectiveDate: z.string().date(),
    eligibilityEvidence: z.string().trim().min(10).max(4000),
    policyEvidence: z.string().trim().min(10).max(4000),
    taxBasis: z.literal("current_tax"),
    inventoryPolicy: z.literal("moving_average"),
    depreciationPolicy: z.literal("straight_line"),
});
export const accountingProfileDecisionSchema = z.object({ expectedVersion: z.number().int().positive(), reason: z.string().trim().min(10).max(4000) });
export type AccountingProfile = Record<string, string | number | null>;
export const financialEventsSchema = z.object({ chartId: z.string().min(1), startDate: z.string().date().optional(), endDate: z.string().date().optional() }).refine(q => !q.startDate || !q.endDate || q.startDate <= q.endDate, { message: 'Report start must not follow its end date.' });
export type ProfileInput = z.infer<typeof accountingProfileSchema>;
export type ProfileDecision = z.infer<typeof accountingProfileDecisionSchema>;
export type FinancialEventsQuery = z.infer<typeof financialEventsSchema>;
