import { z } from 'zod';
import { createJournalSchema } from '@erpx/server-finance/domain/finance';
export const manualDraftEditSchema = createJournalSchema
    .pick({
    chartId: true,
    periodId: true,
    entryDate: true,
    currency: true,
    memo: true,
    lines: true,
})
    .extend({
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().min(10).max(2000),
})
    .strict();
export type ManualDraftInput = z.infer<typeof manualDraftEditSchema>;
export type DraftRow = Record<string, string | number | null>;
export type PreparedDraft = {
    result: {
        fxRateMicros: number;
    };
    preparedLines: Array<{
        accountId: string;
        description?: string;
        debit: number;
        credit: number;
        baseDebit: number;
        baseCredit: number;
        dimensions?: Record<string, string>;
    }>;
};
export type DraftLine = {
    id: string;
    journal_id: string;
    account_id: string;
    description: string;
    debit_minor: number;
    credit_minor: number;
    base_debit_minor: number;
    base_credit_minor: number;
    dimension_json: string;
    ordinal: number;
};
