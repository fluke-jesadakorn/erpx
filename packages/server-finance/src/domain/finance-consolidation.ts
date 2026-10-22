import { z } from "zod";
export type EntityPeriod = {
    companyId: string;
    reportingName: string;
    functionalCurrency: string;
    periodId: string;
    periodName: string;
    endDate: string;
    closeVersion: number;
};
export type ConsolidationRun = Record<string, string | number | null> & {
    id: string;
    holding_id: string;
    presentation_currency: string;
    as_of_date: string;
    status: "draft" | "review" | "approved" | "published";
    source_snapshot_json: string;
    source_hash: string;
    version: number;
    created_by: string;
    reviewed_by: string | null;
    approved_by: string | null;
    published_by: string | null;
};
export type ConsolidationElimination = Record<string, string | number | null> & {
    id: string;
    run_id: string;
    source_transaction_id: string | null;
    debit_account_id: string;
    credit_account_id: string;
    description: string;
    amount_minor: number;
    status: "proposed" | "approved" | "posted";
    version: number;
    approved_by: string | null;
    posted_by: string | null;
    consolidation_journal_id: string | null;
};
export type ConsolidationAccount = {
    id: string;
    company_id: string;
    chart_id: string;
    code: string;
    name: string;
    currency: string;
    base_currency: string;
};
export const consolidationRunSchema = z.object({
    asOfDate: z.string().date(),
    presentationCurrency: z.string().trim().length(3).transform(value => value.toUpperCase()),
    translationRates: z.array(z.object({ currency: z.string().trim().length(3).transform(value => value.toUpperCase()), rate: z.string().regex(/^\d+(\.\d{1,6})?$/) })).max(50).default([]),
});
export const consolidationDecisionSchema = z.object({
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(2000),
});
export const consolidationEliminationSchema = z.object({
    sourceTransactionId: z.string().trim().min(1).max(160),
    debitAccountId: z.string().trim().min(1).max(160),
    creditAccountId: z.string().trim().min(1).max(160),
    amount: z.string().regex(/^\d+(\.\d{1,6})?$/),
    description: z.string().trim().min(3).max(500),
});
export const consolidationEliminationDecisionSchema = z.object({
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(2000),
});
export type EliminationAction = "approve" | "post";
export const eliminationTransition: Record<EliminationAction, {
    from: "proposed" | "approved";
    to: "approved" | "posted";
    permission: string;
    operation: string;
    event: "approved" | "posted";
}> = {
    approve: { from: "proposed", to: "approved", permission: "finance.approve", operation: "finance.consolidation.elimination.approve", event: "approved" },
    post: { from: "approved", to: "posted", permission: "finance.approve", operation: "finance.consolidation.elimination.post", event: "posted" },
};
export const transition = {
    review: { from: "draft", to: "review", permission: "finance.write", operation: "finance.consolidation.review", event: "reviewed" },
    approve: { from: "review", to: "approved", permission: "finance.approve", operation: "finance.consolidation.approve", event: "approved" },
    publish: { from: "approved", to: "published", permission: "finance.approve", operation: "finance.consolidation.publish", event: "published" },
} as const;
