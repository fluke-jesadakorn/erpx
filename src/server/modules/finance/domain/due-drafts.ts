import { z } from "zod";
import { businessBillingSchema } from "./finance-contract-policy";
import { recognitionRunSchema, assetPolicyRunSchema, instrumentRunSchema } from "./due-inputs";
const kind = z.enum(["billing", "recognition", "asset_depreciation", "instrument_payment"]);
export const financeDueDraftSchema = z.object({ kind, sourceId: z.string().min(1), runOn: z.string().date(), input: z.record(z.string(), z.unknown()), reason: z.string().trim().min(3).max(2000) }).strict();
export const financeDueDecisionSchema = z.object({ expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(2000) }).strict();
export type Kind = z.infer<typeof kind>;
export type DueInput = z.infer<typeof financeDueDraftSchema>;
export type DueDecision = z.infer<typeof financeDueDecisionSchema>;
export type Row = Record<string, string | number | null>;
export function parsedInput(k: Kind, input: Record<string, unknown>) { switch (k) {
    case "billing": return businessBillingSchema.parse(input);
    case "recognition": return recognitionRunSchema.parse(input);
    case "asset_depreciation": return assetPolicyRunSchema.parse(input);
    case "instrument_payment": return instrumentRunSchema.parse(input);
} }
