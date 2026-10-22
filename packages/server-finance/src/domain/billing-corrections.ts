import { z } from 'zod';
import { billingRevisionSchema, type BillingRevisionInput } from '@erpx/server-finance/domain/business-billing-revision';
import type { BusinessContractInput } from '@erpx/server-finance/domain/finance-contract-policy';
import { contractBillingCalculation } from '@erpx/server-finance/domain/finance-contract-policy';
export type Row = Record<string, string | number | null>;
export const billingCorrectionSchema = billingRevisionSchema.extend({
    invoiceExpectedVersion: z.number().int().positive(), number: z.string().trim().min(1).max(100),
    periodId: z.string().min(1), entryDate: z.string().date(),
}).strict();
export const billingCorrectionDecisionSchema = z.object({ expectedVersion: z.number().int().positive(), reason: z.string().trim().min(10).max(2000) }).strict();
export type Input = z.infer<typeof billingCorrectionSchema>;
export type CorrectionDecision = z.infer<typeof billingCorrectionDecisionSchema>;
export type CorrectionConfiguration = {
    chart: Row;
    accounts: Row[];
    party: Row;
    taxCode: Row | null;
    dimensions: BusinessContractInput['dimensions'];
};
export type CorrectionSource = {
    bill: Row;
    contract: Row;
    invoice: Row;
    runs: Row[];
    journals: Row[];
    allocations: Row[];
    config: CorrectionConfiguration;
    content: BusinessContractInput;
    result: ReturnType<typeof contractBillingCalculation>;
    sourceHash: string;
};
export type CorrectionResponse = {
    id: string;
    status: string;
    version: number;
    creditJournalId: string;
    replacementBillId: string;
    creditedMinor: number;
    refundMinor: number;
    refundId: string | null;
};
export type ReplacementInput = BillingRevisionInput & {
    parentId: string;
};
export type CreditSummary = {
    result: {
        id: string;
    };
};
