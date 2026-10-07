import {z} from 'zod';export const payrollRemittanceSchema = z.object({
  kind: z.enum(["withholding_tax", "social_security"]),
  amountMinor: z.number().int().positive().safe(),
  expectedRunVersion: z.number().int().positive(),
  bankAccountId: z.string().min(1),
  bankTransactionId: z.string().min(1),
  bankVersion: z.number().int().positive(),
  bankReference: z.string().min(1).max(160),
}).strict();
export type RemittanceKind = z.infer<typeof payrollRemittanceSchema>["kind"];
export type Obligation = { kind: RemittanceKind; accountId: string | null; accruedMinor: number; remittedMinor: number; outstandingMinor: number };
export type PostedRun = { id: string; version: number; currency: string; period_end: string; chart_id: string; journal_id: string };
export const descriptions: Record<RemittanceKind, string> = {
  withholding_tax: "Payroll tax",
  social_security: "Payroll social security",
};
