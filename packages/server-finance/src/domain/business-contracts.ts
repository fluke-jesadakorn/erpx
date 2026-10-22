import { z } from 'zod';
import { businessBillingSchema, contractBillingCalculation, type BusinessContractInput } from '@erpx/server-finance/domain/finance-contract-policy';
import { billingRevisionSchema } from '@erpx/server-finance/domain/business-billing-revision';
export type Row = Record<string, string | number | null>;
export type BillingResult = ReturnType<typeof contractBillingCalculation>;
export const businessDecisionSchema = z.object({
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(2000),
});
export type BillingRevision = z.infer<typeof billingRevisionSchema> & {
    parentId: string;
};
export function billingRevisionRequest(revision: BillingRevision) {
    const { parentId, ...fields } = revision;
    return { parentId, ...billingRevisionSchema.parse(fields) };
}
export function billingHashInput(c: Row, input: z.infer<typeof businessBillingSchema>, result: BillingResult, parent?: Row, content?: BusinessContractInput, reason?: string) {
    return {
        contractId: c.id,
        contractVersion: c.version,
        contractHash: c.source_hash,
        input,
        result,
        ...(parent
            ? {
                parentBillId: parent.id,
                parentVersion: parent.version,
                parentHash: parent.source_hash,
                terms: content,
                reason,
            }
            : {}),
    };
}
export const recognitionReversalSchema = businessDecisionSchema.extend({
    periodId: z.string().min(1),
    entryDate: z.string().date(),
});
