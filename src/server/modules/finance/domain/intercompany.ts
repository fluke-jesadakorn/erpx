import {z} from 'zod';
export type IntercompanyStatus = "unmatched" | "matched" | "settled" | "eliminated";
export const intercompanyTransactionSchema = z.object({
  targetCompanyId: z.string().min(1).max(160),
  reference: z.string().trim().min(2).max(120),
  currency: z.string().length(3).transform(value => value.toUpperCase()),
  amount: z.string().regex(/^\d+(\.\d{1,6})?$/),
  sourceJournalId: z.string().min(1).max(160).optional(),
  targetJournalId: z.string().min(1).max(160).optional(),
});
export const intercompanyDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});
export const groupEntitySchema = z.object({
  companyId: z.string().trim().min(1).max(160),
  reportingName: z.string().trim().min(2).max(160),
  functionalCurrency: z.string().trim().length(3).transform(value => value.toUpperCase()),
});

export type IntercompanyRow = Record<string, string | number | null> & {
  id: string;
  source_company_id: string;
  target_company_id: string;
  reference: string;
  currency: string;
  amount_minor: number;
  status: IntercompanyStatus;
  version: number;
  source_journal_id: string | null;
  target_journal_id: string | null;
  created_by: string;
  created_at: number;
};

export type GroupEntity = {
  company_id: string;
  reporting_name: string;
  functional_currency: string;
  status: string;
  version: number;
  created_by: string;
  updated_at: number;
};

export type DecisionAction = "match" | "settle" | "eliminate";
export const transition: Record<DecisionAction, { from: IntercompanyStatus; to: IntercompanyStatus; permission: string; operation: string; event: string }> = {
  match: { from: "unmatched", to: "matched", permission: "finance.approve", operation: "finance.intercompany.match", event: "matched" },
  settle: { from: "matched", to: "settled", permission: "finance.pay", operation: "finance.intercompany.settle", event: "settled" },
  eliminate: { from: "settled", to: "eliminated", permission: "finance.approve", operation: "finance.intercompany.eliminate", event: "eliminated" },
};

