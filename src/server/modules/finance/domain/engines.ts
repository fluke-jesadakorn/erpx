import { z } from 'zod';
import type { CalculationKind } from './finance-calculations';
const decimal = z.string().regex(/^-?\d+(?:\.\d+)?$/), nonnegative = decimal.refine(value => !value.startsWith("-"));
const base = z.object({ periodId: z.string().min(1).nullable().optional(), currency: z.string().length(3).transform(value => value.toUpperCase()), currencyScale: z.number().int().min(0).max(12).default(2), policyId: z.string().min(1).nullable().optional(), policyVersion: z.number().int().positive().default(1) });
const calculationInput = z.discriminatedUnion("kind", [
    base.extend({ kind: z.literal("allocation"), input: z.object({ total: decimal, weights: z.array(z.object({ key: z.string().min(1), weight: nonnegative })).min(1) }) }),
    base.extend({ kind: z.literal("cash_flow"), input: z.object({ openingCash: decimal, closingCash: decimal, profit: decimal, nonCashAdjustments: decimal, workingCapitalChange: decimal, investing: decimal, financing: decimal, fxEffect: decimal }) }),
    base.extend({ kind: z.literal("consolidation"), input: z.object({ consideration: decimal, previousInterest: decimal.default("0"), fairValueNetAssets: decimal, nciFairValue: decimal.default("0"), parentBps: z.number().int().min(0).max(10000), openingNci: decimal.default("0"), subsidiaryProfit: decimal.default("0"), subsidiaryOci: decimal.default("0"), subsidiaryDividends: decimal.default("0"), nciOwnershipChange: decimal.default("0"), associateOpening: decimal.default("0"), associateBps: z.number().int().min(0).max(10000).default(0), associateProfit: decimal.default("0"), associateOci: decimal.default("0"), associateDividends: decimal.default("0"), associateImpairment: decimal.default("0") }) }),
    base.extend({ kind: z.literal("thai_statutory"), input: z.object({ outputVat: nonnegative, inputVat: nonnegative, disallowedInputVat: nonnegative.default("0"), priorCredit: nonnegative.default("0"), pnd3Withholding: nonnegative.default("0"), pnd53Withholding: nonnegative.default("0"), pp36Vat: nonnegative.default("0"), pnd54Withholding: nonnegative.default("0") }) }),
    base.extend({ kind: z.literal("manufacturing_cost"), input: z.object({ openingQuantityMilli: z.string().regex(/^\d+$/), openingValue: nonnegative, receipts: z.array(z.object({ quantityMilli: z.string().regex(/^\d+$/), value: nonnegative })).default([]), issues: z.array(z.object({ quantityMilli: z.string().regex(/^\d+$/) })).default([]), labor: nonnegative, overhead: nonnegative, abnormalScrapCost: nonnegative.default("0"), outputQuantityMilli: z.string().regex(/^\d+$/) }) }),
    base.extend({ kind: z.literal("reconciliation"), input: z.object({ bankAmount: decimal, allocations: z.array(z.object({ reference: z.string().min(1), amount: decimal })).min(1) }) }),
]);
export const calculationRunSchema = calculationInput;
export const calculationDecisionSchema = z.object({ expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(1000) });
export const labels: Record<CalculationKind, string> = { allocation: "Allocation", cash_flow: "Cash flow", consolidation: "NCI / Equity / Goodwill", thai_statutory: "Thai statutory", manufacturing_cost: "Manufacturing cost", reconciliation: "Reconciliation" };
export type CalculationInput = z.infer<typeof calculationRunSchema>;
export type CalculationRaw = z.input<typeof calculationRunSchema>;
export type CalculationDecision = z.infer<typeof calculationDecisionSchema>;
export type EngineRow = Record<string, string | number | null>;
export type CloseCount = {
    key: string;
    count: number;
};
export type CloseCheck = CloseCount & {
    status: 'pass' | 'block';
};
