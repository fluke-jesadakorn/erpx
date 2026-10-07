import { z } from 'zod';
import { currencyDefinition } from '@/lib/company-money';
const code = z.string().regex(/^[A-Z]{3}$/).refine((value) => Boolean(currencyDefinition(value)), "Use a current ISO 4217 code.");
export const preferenceSchema = z.object({ currency: code });
export const manualRateSchema = z.object({
    currency: code.refine((value) => value !== "USD", "USD has a fixed rate of 1."),
    effectiveDate: z.iso.date(),
    currencyPerUsd: z.string().regex(/^\d{1,20}(?:\.\d{1,18})?$/, "Enter a positive decimal rate with at most 20 whole digits and 18 decimal places."),
    note: z.string().trim().max(500).default(""),
});
export const currencyScaleSchema = z.object({ currency: code, scale: z.number().int().min(0).max(12) });
export const conversionSchema = z.object({
    amount: z.string().regex(/^-?\d{1,26}(?:\.\d{1,12})?$/),
    currency: code,
    targetCurrency: code,
    transactionDate: z.iso.date(),
    requireRate: z.boolean().default(false),
});
export const moneyIdentitySchema = z.object({
    sourceType: z.string().trim().min(1).max(100),
    sourceId: z.string().trim().min(1).max(200),
    amountKind: z.string().trim().min(1).max(100),
    transactionDate: z.iso.date(),
    amountUnits: z.string().regex(/^-?\d{1,38}$/),
});
export type MoneyIdentity = {
    sourceType: string;
    sourceId: string;
    amountKind: string;
};
export type SnapshotInput = MoneyIdentity & {
    transactionDate: string;
    currency: string;
    amountUnits: bigint | string;
    currencyScale?: number;
};
export type CarryInput = MoneyIdentity & {
    transactionDate: string;
    amountUnits: bigint | string;
    allocationOffsetUnits?: bigint | string;
    original: MoneyIdentity;
};
export type ReversalInput = MoneyIdentity & {
    transactionDate: string;
    amountUnits: bigint | string;
    original: MoneyIdentity;
};
export type SnapshotSource = {
    currency: string;
    currency_scale: number;
    amount_units: string;
    usd_units: string;
    fx_rate_id: string | null;
    used_latest_fallback: boolean;
};
export type CarrySource = SnapshotSource & {
    original_source_type: string | null;
};
export type SnapshotPlan = {
    currency: string;
    amountUnits: string;
    usdUnits: string;
    currencyScale: number;
    rateId: string | null;
    usedLatestFallback: boolean;
    capturedAt: number;
};
export type CarryPlan = SnapshotPlan & {
    allocationOffsetUnits: string | null;
};
