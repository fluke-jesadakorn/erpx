import {z} from 'zod';
const dimensionValues = z.record(
  z.string().min(1).max(80),
  z.string().min(1).max(160),
);
const planBase = {
  chartId: z.string().min(1),
  name: z.string().trim().min(1).max(160),
  parentId: z.string().min(1).optional(),
  expectedVersion: z.number().int().positive().optional(),
};
export const budgetEditionSchema = z.object({
  ...planBase,
  year: z.number().int().min(2000).max(2199),
  lines: z
    .array(
      z.object({
        accountId: z.string().min(1),
        month: z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/),
        amount: z.string(),
        dimensions: dimensionValues.default({}),
      }),
    )
    .min(1)
    .max(10000),
});
export const forecastEditionSchema = z.object({
  ...planBase,
  startMonth: z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/),
  assumptions: z.string().trim().min(3).max(4000),
  lines: z
    .array(
      z.object({
        accountId: z.string().min(1),
        monthlyAmount: z.string(),
        monthlyGrowthBps: z.number().int().min(-10000).max(10000).default(0),
        goodBps: z.number().int().min(-10000).max(100000).default(1000),
        badBps: z.number().int().min(-10000).max(100000).default(-1000),
        dimensions: dimensionValues.default({}),
      }),
    )
    .min(1)
    .max(500),
});
export const reportRowSchema = z.discriminatedUnion("kind", [
  z.object({
    key: z.string().regex(/^[a-z][a-z0-9_]{0,39}$/),
    label: z.string().trim().min(1).max(160),
    kind: z.literal("accounts"),
    accountIds: z.array(z.string().min(1)).min(1).max(2000),
    sign: z.enum(["debit", "credit"]).default("debit"),
    basis: z.enum(["cumulative", "period"]).default("period"),
    dimensions: dimensionValues.default({}),
  }),
  z.object({
    key: z.string().regex(/^[a-z][a-z0-9_]{0,39}$/),
    label: z.string().trim().min(1).max(160),
    kind: z.enum(["sum", "difference", "ratio"]),
    references: z.array(z.string().min(1)).min(1).max(100),
  }),
]);
export const financialReportSchema = z
  .object({
    ...planBase,
    rows: z.array(reportRowSchema).min(1).max(200),
    cashFlow: z
      .object({
        bankAccountIds: z.array(z.string().min(1)).min(1),
        classifications: z
          .array(
            z.object({
              accountId: z.string().min(1),
              kind: z.enum(["operating", "investing", "financing"]),
            }),
          )
          .min(1),
      })
      .optional(),
  })
  .superRefine((v, c) => {
    const keys = new Set<string>(),units = new Map<string,string>();
    for (const r of v.rows) {
      if (keys.has(r.key))
        c.addIssue({ code: "custom", message: "Report keys must be unique." });
      if (r.kind !== "accounts" && r.references.some((k) => !keys.has(k)))
        c.addIssue({
          code: "custom",
          message: "Formula references must point to earlier rows.",
        });
      if (
        (r.kind === "ratio" || r.kind === "difference") &&
        r.references.length !== 2
      )
        c.addIssue({
          code: "custom",
          message: "Difference and ratio need exactly two rows.",
        });
      if(r.kind!=="accounts"&&r.references.some(k=>units.get(k)!=="money"))c.addIssue({code:"custom",message:"Sum, difference and ratio operands must be money rows."});
      units.set(r.key,r.kind==="ratio"?"percent":"money");
      keys.add(r.key);
    }
    if (v.cashFlow) {
      const ids = v.cashFlow.classifications.map((r) => r.accountId);
      if (
        new Set(ids).size !== ids.length ||
        ids.some((id) => v.cashFlow!.bankAccountIds.includes(id))
      )
        c.addIssue({
          code: "custom",
          message: "Classify each nonbank account once.",
        });
    }
  });
export type BudgetContent = {
  year: number;
  lines: {
    accountId: string;
    month: string;
    minor: number;
    dimensions: Record<string, string>;
  }[];
};
export type PlanRow = {
  id: string;
  company_id: string;
  chart_id: string;
  kind: string;
  name: string;
  parent_id: string | null;
  edition: number;
  status: string;
  version: number;
  content_json: string;
  content_hash: string;
  prepared_by: string;
};
export type PlanAccount = { id: string; code: string; name: string; category: string };
export type LedgerLine = {
  id: string;
  journalId: string;
  accountId: string;
  entryDate: string;
  debit: number;
  credit: number;
  dimensions: string;
  sourceType: string;
  isFiscalClose?: number;
  sourceId: string | null;
};
export type RetainedReport={rows:{key:string;label:string;value:string|null;minor:number|null;unit:string;journalIds:string[]}[];cashFlow?:Record<string,unknown>|null};
export type PlanningSourceSnapshot={result_json:string;content_json:string;source_hash:string;chart_id:string;start_date:string;end_date:string};
