import { z } from 'zod';
import { businessContractSchema, businessBillingSchema } from './finance-contract-policy';
export const billingRevisionSchema = z
    .object({
    expectedVersion: z.number().int().positive(),
    invoiceExpectedVersion: z.number().int().positive().optional(),
    billing: businessBillingSchema.omit({ expectedVersion: true }),
    pricing: businessContractSchema.shape.pricing.optional(),
    obligations: businessContractSchema.shape.obligations.optional(),
    reason: z.string().trim().min(3).max(2000),
})
    .strict();
export type BillingRevisionInput = z.infer<typeof billingRevisionSchema>;
