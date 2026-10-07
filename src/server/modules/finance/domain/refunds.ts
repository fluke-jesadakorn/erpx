import { z } from 'zod';
import type { FinanceError } from './invoice-line-revision';
export type Row = Record<string, string | number | null>;
const version = z.number().int().positive();
const reason = z.string().trim().min(3).max(1000);
export const refundApprovalSchema = z.object({ expectedVersion: version, reason, beneficiaryName: z.string().trim().min(2).max(160), beneficiaryAccount: z.string().trim().min(3).max(160) });
export const refundInstructionSchema = z.object({ expectedVersion: version, amount: z.string().regex(/^\d+(\.\d{1,2})?$/), plannedDate: z.iso.date(), bankAccount: z.string().trim().min(1).max(160), cashAccountId: z.string().min(1), fxGainAccountId: z.string().min(1).optional(), fxLossAccountId: z.string().min(1).optional() });
export const refundExportSchema = z.object({ instructions: z.array(z.object({ id: z.string().min(1), expectedVersion: version })).min(1).max(100) });
export const refundConfirmationSchema = z.object({ expectedVersion: version, bankTransactionId: z.string().min(1), bankVersion: version, periodId: z.string().min(1), bankReference: z.string().trim().min(2).max(160), evidence: reason, attachmentSetIds: z.array(z.string().min(1)).max(20).default([]) });
export const refundExceptionSchema = z.object({ expectedVersion: version, outcome: z.enum(['unknown', 'failed', 'cancelled']), evidence: reason });
export type RefundApproval = z.infer<typeof refundApprovalSchema>;
export type RefundInstruction = z.infer<typeof refundInstructionSchema>;
export type RefundExport = z.infer<typeof refundExportSchema>;
export type RefundConfirmation = z.infer<typeof refundConfirmationSchema>;
export type RefundException = z.infer<typeof refundExceptionSchema>;
export type RefundProof = {
    id: string;
    purpose_key: string;
    component_kind: string;
    component_ref: string | null;
};
export type RefundJournal = {
    result: {
        id: string;
        fxRateMicros: number;
    };
};
export const csvCell = (value: unknown) => `"${String(value ?? '').replace(/^[=+@\-\t\r]/, "'$&").replaceAll('"', '""')}"`;
export function createRefundPolicy(error: FinanceError) {
    function checkVersion(row: Row, expected: number) {
        if (Number(row.version) !== expected)
            throw error(409, 'VERSION_CONFLICT', 'The record changed. Refresh before continuing.');
    }
    function proportionalCarrying(base: number, outstanding: number, amount: number) {
        if (!Number.isSafeInteger(base) || base < 0 || !Number.isSafeInteger(outstanding) || outstanding <= 0 || amount > outstanding)
            throw error(409, 'REFUND_CARRYING_EVIDENCE_REQUIRED', 'Reconcile the refund source carrying value before confirming cash.');
        if (amount === outstanding)
            return base;
        return Number((BigInt(base) * BigInt(amount) + BigInt(Math.floor(outstanding / 2))) / BigInt(outstanding));
    }
    return { checkVersion, proportionalCarrying };
}
