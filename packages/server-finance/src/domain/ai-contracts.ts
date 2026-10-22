import { z } from "zod";

export const financeAiKinds = ["today_brief", "invoice_review", "collection_draft", "payment_preparation", "bank_match", "tax_review", "close_review", "report_explanation", "receipt_extraction"] as const;
export type FinanceAiKind = typeof financeAiKinds[number];
export const financeAiLabels: Record<FinanceAiKind, string> = {
  today_brief: "Daily Finance brief", invoice_review: "Document coding & posting review", collection_draft: "Collection drafts", payment_preparation: "Payment preparation", bank_match: "Bank match suggestions", tax_review: "Tax evidence review", close_review: "Period close review", report_explanation: "Report explanations", receipt_extraction: "Receipt extraction",
};
const timezone = z.string().max(100).refine(value => { try { new Intl.DateTimeFormat("en", { timeZone: value }); return true; } catch { return false; } }, "Choose a valid IANA timezone.");
export const financeAiPolicySchema = z.object({
  expectedVersion: z.number().int().nonnegative(), enabled: z.boolean(),
  kinds: z.array(z.enum(financeAiKinds)).max(financeAiKinds.length).refine(values => new Set(values).size === values.length, "Choose each capability once."),
  timezone: timezone.default("Asia/Bangkok"), dailyLimit: z.number().int().min(1).max(50).default(10), monthlyLimit: z.number().int().min(1).max(1000).default(100), reason: z.string().trim().min(10).max(2000),
}).refine(value => !value.enabled || value.kinds.length > 0, "Choose at least one capability.");
export const financeAiPrepareSchema = z.object({
  kind: z.enum(financeAiKinds).exclude(["receipt_extraction"]), sourceId: z.string().min(1).max(128),
  retry: z.boolean().default(false), periodId: z.string().min(1).max(128).optional(), chartId:z.string().min(1).max(128).optional(), filingMonth:z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/).optional(), startDate: z.string().date().optional(), endDate: z.string().date().optional(),
});
export const financeAiResultSchema = z.object({
  summary: z.string().trim().min(1).max(2500), checks: z.array(z.string().trim().min(1).max(500)).max(8),
  draftText: z.string().max(8000).nullable(),
  accounts: z.object({ debitAccountId: z.string().min(1).max(128), creditAccountId: z.string().min(1).max(128), taxAccountId: z.string().max(128).nullable() }).nullable(),
  bankPaymentId: z.string().max(128).nullable(),
  citations: z.array(z.object({ sourceId: z.string().min(1).max(180), field: z.string().min(1).max(100) })).min(1).max(12),
});
export type FinanceAiResult = z.infer<typeof financeAiResultSchema>;
export const financeAiDecisionSchema = z.object({ expectedVersion: z.number().int().positive(), reason: z.string().trim().min(10).max(2000) });
export const financeAiApplySchema = financeAiDecisionSchema.extend({ accounts: z.object({ debitAccountId: z.string().min(1), creditAccountId: z.string().min(1), taxAccountId: z.string().nullable() }) });
export function financeAiDateAt(timezone: string, now: Date) { return new Intl.DateTimeFormat("en-CA", { timeZone: timezone, year: "numeric", month: "2-digit", day: "2-digit" }).format(now); }
