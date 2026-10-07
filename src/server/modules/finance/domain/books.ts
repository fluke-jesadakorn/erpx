import {z} from 'zod';
export const dimensionSchema = z
  .object({
    code: z.string().regex(/^[a-z][a-z0-9_]{0,39}$/),
    name: z.string().trim().min(1).max(160),
    required: z.boolean().default(false),
    values: z
      .array(
        z.object({
          code: z.string().trim().min(1).max(160),
          name: z.string().trim().min(1).max(160),
        }),
      )
      .min(1)
      .max(1000),
  })
  .superRefine((v, ctx) => {
    if (new Set(v.values.map((x) => x.code)).size !== v.values.length)
      ctx.addIssue({
        code: "custom",
        message: "Dimension values must be unique.",
      });
  });
export const bookRuleSchema = z
  .object({
    primaryChartId: z.string().min(1),
    secondaryChartId: z.string().min(1),
    effectiveDate: z.string().date(),
    openingPeriodId: z.string().min(1),
    reason: z.string().trim().min(3).max(2000),
    mappings: z
      .array(
        z.object({
          primaryAccountId: z.string().min(1),
          secondaryAccountId: z.string().min(1),
        }),
      )
      .min(2)
      .max(2000),
  })
  .superRefine((v, c) => {
    if (
      new Set(v.mappings.map((x) => x.primaryAccountId)).size !==
        v.mappings.length ||
      new Set(v.mappings.map((x) => x.secondaryAccountId)).size !==
        v.mappings.length
    )
      c.addIssue({
        code: "custom",
        message: "Map each source and target account once.",
      });
  });

export type OpeningRow={accountId:string;dimensions:string;minor:number};
export type BookAccount={id:string;chart_id:string;category:string;control_kind:string;currency:string|null;status:string};
export type BookRule={status:string;version:number;primary_chart_id:string;secondary_chart_id:string;effective_date:string;opening_json:string;opening_hash:string;opening_period_id:string;prepared_by:string};
export type BookWorkspace={definitions:Record<string,unknown>[];values:Record<string,unknown>[];rules:Record<string,unknown>[];mappings:Record<string,unknown>[];charts:Record<string,unknown>[];accounts:Record<string,unknown>[];periods:Record<string,unknown>[]};
