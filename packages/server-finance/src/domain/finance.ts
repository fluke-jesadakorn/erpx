import { z } from 'zod';
import { invoiceLineSchema } from '@erpx/server-finance/domain/finance-invoice-lines';
export const moneySchema = z.string().regex(/^-?\d+(\.\d{1,6})?$/, "Use a decimal money amount.");
export const lineSchema = z.object({
    accountId: z.string().min(1).max(128),
    description: z.string().max(500).optional(),
    debit: moneySchema,
    credit: moneySchema,
    dimensions: z.record(z.string().max(80), z.string().max(160)).optional(),
});
export const createJournalSchema = z.object({
    chartId: z.string().min(1).max(128),
    periodId: z.string().min(1).max(128),
    entryDate: z.string().date(),
    currency: z.string().length(3).transform((value) => value.toUpperCase()),
    fxRate: z.string().optional(),
    memo: z.string().max(2000).default(""),
    sourceType: z.string().max(64).default("manual"),
    sourceId: z.string().max(128).optional(),
    isOpening: z.boolean().default(false),
    lines: z.array(lineSchema).min(2).max(500),
});
export const openingTrialBalanceSchema = z.object({
    chartId: z.string().min(1).max(128),
    periodId: z.string().min(1).max(128),
    entryDate: z.string().date(),
    currency: z.string().length(3).transform((value) => value.toUpperCase()),
    lines: z.array(lineSchema.pick({ accountId: true, description: true, debit: true, credit: true }).extend({
        subledgerKind: z.enum(["none", "receivable", "payable"]).default("none"),
    })).min(2).max(500),
    subledgerItems: z.array(z.object({
        kind: z.enum(["receivable", "payable"]),
        partyId: z.string().min(1).max(128),
        reference: z.string().trim().min(1).max(120),
        dueDate: z.string().date(),
        controlAccountId: z.string().min(1).max(128),
        offsetAccountId: z.string().min(1).max(128),
        amount: moneySchema,
    })).max(2000).default([]),
});
export const createChartSchema = z.object({
    name: z.string().trim().min(1).max(160),
    baseCurrency: z.string().length(3).transform((value) => value.toUpperCase()),
    accountingBasis: z.string().trim().max(40).default("IFRS"),
});
export const createAccountSchema = z.object({
    chartId: z.string().min(1),
    code: z.string().trim().min(1).max(40),
    name: z.string().trim().min(1).max(160),
    category: z.enum(["asset", "liability", "equity", "revenue", "expense", "other_income", "other_expense"]),
    parentId: z.string().max(128).nullable().optional(),
    currency: z.string().length(3).transform((value) => value.toUpperCase()).optional(),
    isControl: z.boolean().default(false),
    controlKind: z.enum(["none", "receivable", "payable", "bank"]).default("none"),
});
export const changeAccountStatusSchema = z.object({
    status: z.enum(["active", "inactive"]),
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(2000),
});
export const createPeriodSchema = z.object({
    name: z.string().trim().min(1).max(100),
    startDate: z.string().date(),
    endDate: z.string().date(),
});
export const closeProofSchema = z.object({
    proof: z.string().trim().min(1).max(2000),
    expectedVersion: z.number().int().nonnegative().optional(),
    attachmentSetIds: z.array(z.string().min(1)).max(20).default([]),
});
export const reversePaymentSchema = z.object({
    expectedVersion: z.number().int().positive(),
    periodId: z.string().min(1),
    entryDate: z.string().date(),
    reason: z.string().trim().min(3).max(500),
});
export const journalBatchSchema = z.object({
    journals: z.array(z.object({ id: z.string().min(1).max(128), expectedVersion: z.number().int().positive() })).min(1).max(100),
    reason: z.string().trim().min(10).max(2000),
});
export const reportDateSchema = z.object({ startDate: z.string().date().optional(), endDate: z.string().date().optional() })
    .refine(value => !value.startDate || !value.endDate || value.startDate <= value.endDate, { message: "Report start must not follow its end date." });
export const prepareFxRevaluationSchema = z.object({
    chartId: z.string().min(1), periodId: z.string().min(1), entryDate: z.string().date(),
    rates: z.array(z.object({ currency: z.string().length(3).transform(value => value.toUpperCase()), rate: z.string() })).min(1).max(50),
    gainAccountId: z.string().min(1), lossAccountId: z.string().min(1), proof: z.string().trim().min(3).max(2000),
});
export const decideFxRevaluationSchema = z.object({ expectedVersion: z.number().int().positive(), proof: z.string().trim().min(3).max(2000) });
export const cancelCloseSchema = z.object({ expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(2000) });
export const partySchema = z.object({ kind: z.enum(["customer", "supplier", "employee"]), code: z.string().min(1).max(64), name: z.string().min(1).max(200), currency: z.string().length(3).transform((v) => v.toUpperCase()), taxRegistration: z.string().max(100).optional(), paymentTermsDays: z.number().int().min(0).max(365).default(30), creditLimit: moneySchema.default("0") });
export const invoiceSchema = z.object({ expectedVersion: z.number().int().positive().optional(), aiProposalId: z.string().max(128).optional(), aiProposalVersion: z.number().int().positive().optional(), aiReason: z.string().trim().min(10).max(2000).optional(), partyId: z.string().min(1), kind: z.enum(["payable", "receivable"]), invoiceNumber: z.string().min(1).max(100), invoiceDate: z.string().date(), dueDate: z.string().date(), currency: z.string().length(3).transform((v) => v.toUpperCase()), subtotal: moneySchema, tax: moneySchema.default("0"), taxCode: z.string().max(64).optional(), taxAccountId: z.string().min(1).optional(), sourceType: z.string().max(64).optional(), sourceId: z.string().max(128).optional(), debitAccountId: z.string().min(1), creditAccountId: z.string().min(1), lines: z.array(invoiceLineSchema).min(1).max(100).optional() });
export const invoicePostingSchema = z.object({
    periodId: z.string().min(1), entryDate: z.iso.date(), expectedVersion: z.number().int().positive().optional(),
    fxRate: moneySchema.optional(), attachmentSetIds: z.array(z.string().min(1)).max(20).optional(),
    fxGainAccountId: z.string().min(1).optional(), fxLossAccountId: z.string().min(1).optional(),
});
export const confirmPaymentSchema = z.object({
    expectedVersion: z.number().int().positive(), periodId: z.string().min(1), bankTransactionId: z.string().min(1), bankVersion: z.number().int().positive(),
    bankReference: z.string().trim().min(2).max(160), proof: z.string().trim().min(3).max(1000),
    fxRate: z.string().optional(), fxGainAccountId: z.string().min(1).optional(), fxLossAccountId: z.string().min(1).optional(),
    allocations: z.array(z.object({ invoiceId: z.string().min(1), amount: moneySchema })).min(1).max(100),
    attachmentSetIds: z.array(z.string().min(1)).max(20).default([]),
});
export const taxCodeSchema = z.object({ code: z.string().min(1).max(40), name: z.string().min(1).max(160), rate: z.string(), recoverable: z.string().default("1"), effectiveFrom: z.string().date(), effectiveTo: z.string().date().optional() });
export const budgetSchema = z.object({ name: z.string().min(1).max(160), fiscalYear: z.number().int().min(2000).max(2200), currency: z.string().length(3).transform((v) => v.toUpperCase()) });
export const leaseSchema = z.object({ leaseNumber: z.string().min(1).max(64), counterparty: z.string().min(1).max(200), currency: z.string().length(3).transform((v) => v.toUpperCase()), commencementDate: z.string().date(), endDate: z.string().date(), payment: moneySchema, discountRate: z.string().default("0"), paymentFrequency: z.string().default("monthly") });
export const loanSchema = z.object({ loanNumber: z.string().min(1).max(64), lender: z.string().min(1).max(200), currency: z.string().length(3).transform((v) => v.toUpperCase()), principal: moneySchema, annualRate: z.string().default("0"), commencementDate: z.string().date(), maturityDate: z.string().date() });
export const revenueContractSchema = z.object({ customerId: z.string(), contractNumber: z.string().min(1).max(64), currency: z.string().length(3).transform((v) => v.toUpperCase()), transactionPrice: moneySchema });
export const revenueObligationSchema = z.object({ contractId: z.string(), description: z.string().min(1).max(200), allocated: moneySchema, recognitionMethod: z.enum(["point_in_time", "over_time", "milestone"]), startDate: z.string().date(), endDate: z.string().date().optional(), revenueAccountId: z.string(), deferredAccountId: z.string() });
export type JournalLineInput = {
    accountId: string;
    description?: string;
    debit: string;
    credit: string;
    dimensions?: Record<string, string>;
};
export type FinanceRow = Record<string, string | number | null>;
export type ReportDates = z.infer<typeof reportDateSchema>;
export type CloseReconciliation = {
    kind: "receivable" | "payable" | "bank";
    accountId: string;
    accountCode: string;
    accountName: string;
    chartName: string;
    currency: string;
    ledgerMinor: number;
    sourceMinor: number;
    differenceMinor: number;
    source: Record<string, unknown>;
};
export type FxExposure = {
    invoiceId: string;
    kind: "receivable" | "payable";
    currency: string;
    controlAccountId: string;
    transactionOutstanding: number;
    priorBase: number;
    revaluedBase: number;
    adjustment: number;
    rateMicros: number;
    version: number;
};
export type JournalInput = z.infer<typeof createJournalSchema>;
export type FunctionalFxEvidence = {
    micros: number;
    sourceRateId: string | null;
    targetRateId: string | null;
    usedLatestFallback: boolean;
};
export type PreparedJournalLine = Omit<JournalLineInput, 'debit' | 'credit'> & {
    debit: number;
    credit: number;
    baseDebit: number;
    baseCredit: number;
};
export type HistoricalAgingRow = Record<string, string | number | null | boolean> & {
    id:string;source_id:string|null;
    legacy_date_evidence: boolean;
};
