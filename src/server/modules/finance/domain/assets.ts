import { z } from "zod";
import { decimalToMinor } from "@/lib/finance/finance-money";
import type { FinanceError } from "./invoice-line-revision";
const moneySchema = z.string().regex(/^-?\d+(\.\d{1,6})?$/, "Use a decimal money amount.");
export const assetSchema = z.object({
    assetNumber: z.string().trim().min(1).max(64), name: z.string().trim().min(1).max(200), accountId: z.string().min(1),
    depreciationAccountId: z.string().min(1).optional(), depreciationExpenseAccountId: z.string().min(1).optional(),
    currency: z.string().length(3).transform(v => v.toUpperCase()).default("USD"), cost: moneySchema, residual: moneySchema.default("0"),
    usefulLifeMonths: z.number().int().positive().max(1200), inServiceDate: z.string().date(),
});
export const assetEditSchema = assetSchema.extend({ expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(2000) });
const assetDimensionsSchema = z.record(z.string().max(80), z.string().max(160)).optional();
export const assetActivationSchema = z.object({ expectedVersion: z.number().int().positive(), periodId: z.string().min(1), entryDate: z.string().date(), offsetAccountId: z.string().min(1), evidenceRef: z.string().trim().min(3).max(2000), dimensions: assetDimensionsSchema });
export const assetDepreciationSchema = z.object({ expectedVersion: z.number().int().positive(), periodId: z.string().min(1), entryDate: z.string().date(), dimensions: assetDimensionsSchema });
export const assetDecisionSchema = z.object({ expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(2000) });
export const assetDisposalSchema = z.object({ expectedVersion: z.number().int().positive(), periodId: z.string().min(1), entryDate: z.string().date(), proceeds: moneySchema.default("0"), proceedsAccountId: z.string().min(1).optional(), gainAccountId: z.string().min(1), lossAccountId: z.string().min(1), evidenceRef: z.string().trim().min(3).max(2000) });
export type Asset = {
    id: string;
    company_id: string;
    asset_number: string;
    name: string;
    account_id: string;
    depreciation_account_id: string | null;
    depreciation_expense_account_id: string | null;
    currency: string;
    cost_minor: number;
    residual_minor: number;
    useful_life_months: number;
    in_service_date: string;
    accumulated_depreciation_minor: number;
    activation_journal_id: string | null;
    status: string;
    created_by: string;
    version: number;
};
export type Disposal = {
    id: string;
    asset_id: string;
    company_id: string;
    asset_version: number;
    period_id: string;
    entry_date: string;
    proceeds_minor: number;
    proceeds_account_id: string | null;
    gain_account_id: string;
    loss_account_id: string;
    status: string;
    created_by: string;
    version: number;
};
export type AssetInput = z.infer<typeof assetSchema>;
export type AssetEdit = z.infer<typeof assetEditSchema>;
export type AssetActivation = z.infer<typeof assetActivationSchema>;
export type AssetDepreciation = z.infer<typeof assetDepreciationSchema>;
export type AssetDecision = z.infer<typeof assetDecisionSchema>;
export type AssetDisposal = z.infer<typeof assetDisposalSchema>;
export type AssetRow = Record<string, unknown>;
export type AssetAccount = {
    id: string;
    chart_id: string;
    category: string;
};
export type AssetPolicyDisposal = {
    hash: string | null;
    dimensions: Record<string, string>;
    journalIds: string[];
};
export type AssetPolicyDisposalInput = {
    id: string;
    periodId: string;
    entryDate: string;
    proceeds: number;
    gainAccountId: string;
    lossAccountId: string;
};
export function createAssetValuePolicy(error: FinanceError) {
    function version(asset: {
        version: number;
    }, expected: number) { if (asset.version !== expected)
        throw error(409, "ASSET_CHANGED", "Reload and review the current asset revision."); }
    function values(input: z.infer<typeof assetSchema>) { const cost = decimalToMinor(input.cost), residual = decimalToMinor(input.residual); if (cost <= 0 || residual < 0 || residual > cost)
        throw error(422, "ASSET_VALUE_INVALID", "Cost must be positive and residual must be between zero and cost."); return { cost, residual }; }
    function monthOrdinal(date: string) { return Number(date.slice(0, 4)) * 12 + Number(date.slice(5, 7)) - 1; }
    return { version, values, monthOrdinal };
}
