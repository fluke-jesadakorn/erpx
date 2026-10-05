import { fiscalCloseSourceSql } from "./finance-fiscal-close-source";
import { financeTransaction } from "./finance-transactions";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { isTransactionalSqlDatabase } from "./runtime/types";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { executeCommand, replayCommand, assertion } from "./commands";
import { ApiError } from "./http";
import { samePerson } from "@/lib/iam";
import { sha256 } from "./crypto";
import {
  decimalToMinor,
  minorToDecimal,
  multiplyBps,
} from "@/lib/finance-money";
import { financeCsvRows, financeCsv } from "@/lib/finance-csv";
import { financeReviewSchema } from "./finance-books";
import { requireFinanceTool } from "./finance-company-tools";
import { requireFinanceMoneyScale } from "./finance-money-policy";
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
const reportRowSchema = z.discriminatedUnion("kind", [
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
type BudgetContent = {
  year: number;
  lines: {
    accountId: string;
    month: string;
    minor: number;
    dimensions: Record<string, string>;
  }[];
};
type PlanRow = {
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
type Account = { id: string; code: string; name: string; category: string };
function money(s: string) {
  try {
    return decimalToMinor(s);
  } catch {
    throw new ApiError(
      422,
      "INVALID_MONEY",
      "Enter an exact two-decimal money amount.",
    );
  }
}
function safe(n: bigint) {
  if (
    n > BigInt(Number.MAX_SAFE_INTEGER) ||
    n < BigInt(Number.MIN_SAFE_INTEGER)
  )
    throw new ApiError(
      422,
      "MONEY_OVERFLOW",
      "Amount exceeds supported precision.",
    );
  return Number(n);
}
const canonicalDimensions = (d: Record<string, string>) =>
  JSON.stringify(
    Object.fromEntries(
      Object.entries(d).sort(([a], [b]) => a.localeCompare(b)),
    ),
  );
async function accounts(
  db: SqlDatabase,
  actor: WorkContext,
  chart: string,
  ids: string[],
  dimensions: Record<string, string>[] = [],
) {
  const book = await db
    .prepare(
      "SELECT base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'",
    )
    .bind(chart, actor.companyId)
    .first<{base_currency:string}>();
  if (!book)
    throw new ApiError(
      404,
      "CHART_NOT_FOUND",
      "Select an active Company book.",
    );
  await requireFinanceMoneyScale(db,actor.companyId,book.base_currency);
  const rows = (
    await db
      .prepare(
        "SELECT id,code,name,category FROM finance_accounts WHERE chart_id=? AND company_id=? AND status='active'",
      )
      .bind(chart, actor.companyId)
      .all<Account>()
  ).results;
  if (ids.some((id) => !rows.some((a) => a.id === id)))
    throw new ApiError(
      422,
      "ACCOUNT_INVALID",
      "Every account must belong to the selected Company book.",
    );
  for (const dims of dimensions)
    for (const [code, value] of Object.entries(dims))
      if (
        !(await db
          .prepare(
            "SELECT 1 FROM finance_dimension_definitions d JOIN finance_dimension_values v ON v.dimension_id=d.id WHERE d.company_id=? AND d.code=? AND d.status='active' AND v.code=? AND v.status='active'",
          )
          .bind(actor.companyId, code, value)
          .first())
      )
        throw new ApiError(
          422,
          "DIMENSION_INVALID",
          "Select an active Company dimension value.",
        );
  return rows;
}
async function save(
  actor: WorkContext,
  kind: "budget" | "forecast" | "report",
  base:
    | z.infer<typeof budgetEditionSchema>
    | z.infer<typeof forecastEditionSchema>
    | z.infer<typeof financialReportSchema>,
  content: unknown,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  const { write } = await holdingResources(actor.holdingId),
    op = `finance.${kind}.prepare`,
    input = { ...base, content };
  const receipt = await write
    .prepare("SELECT record_id FROM command_receipts WHERE command_id=?")
    .bind(key)
    .first<{ record_id: string }>();
  if (receipt) {
    const prior = await replayCommand(
      write,
      actor,
      key,
      op,
      receipt.record_id,
      input,
    );
    if (prior) return prior;
  }
  await requireFinanceTool(write, actor.companyId, "planning");
  let edition = 1;
  const guards = [];
  if (base.parentId) {
    const parent = await write
      .prepare(
        "SELECT * FROM finance_plan_editions WHERE id=? AND company_id=? AND chart_id=? AND kind=? AND status='approved'",
      )
      .bind(base.parentId, actor.companyId, base.chartId, kind)
      .first<PlanRow>();
    if (
      !parent ||
      parent.version !== base.expectedVersion ||
      parent.name !== base.name
    )
      throw new ApiError(
        409,
        "STALE_VERSION",
        "Select the current approved edition and its version to amend.",
      );
    edition = parent.edition + 1;
    guards.push(
      assertion(
        write,
        key,
        "parent",
        "EXISTS(SELECT 1 FROM finance_plan_editions WHERE id=? AND company_id=? AND version=? AND status='approved')",
        [base.parentId, actor.companyId, base.expectedVersion!],
      ),
    );
  }
  const id = crypto.randomUUID(),
    hash = await sha256(JSON.stringify(content)),
    now = Date.now();
  return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    1,
    { id, version: 1, status: "draft", edition, contentHash: hash },
    [
      ...guards,
      write
        .prepare(
          "INSERT INTO finance_plan_editions(id,company_id,chart_id,kind,name,parent_id,edition,content_json,content_hash,prepared_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          base.chartId,
          kind,
          base.name,
          base.parentId ?? null,
          edition,
          JSON.stringify(content),
          hash,
          actor.userId,
          now,
        ),
    ],
  );
}
export async function prepareBudgetEdition(
  actor: WorkContext,
  input: z.infer<typeof budgetEditionSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  input = budgetEditionSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId);
  await accounts(
    write,
    actor,
    input.chartId,
    input.lines.map((l) => l.accountId),
    input.lines.map((l) => l.dimensions),
  );
  const seen = new Set<string>();
  const lines = input.lines.map((l) => {
    if (!l.month.startsWith(`${input.year}-`))
      throw new ApiError(
        422,
        "BUDGET_YEAR_MISMATCH",
        "All months must be in the budget year.",
      );
    const id = `${l.accountId}:${l.month}:${canonicalDimensions(l.dimensions)}`;
    if (seen.has(id))
      throw new ApiError(
        422,
        "DUPLICATE_BUDGET_LINE",
        "Each account/month/dimension combination must be unique.",
      );
    seen.add(id);
    return {
      accountId: l.accountId,
      month: l.month,
      minor: money(l.amount),
      dimensions: JSON.parse(canonicalDimensions(l.dimensions)) as Record<
        string,
        string
      >,
    };
  });
  return save(actor, "budget", input, { year: input.year, lines }, key);
}
export async function importBudgetCsv(
  actor: WorkContext,
  input: {
    chartId: string;
    name: string;
    year: number;
    csv: string;
    parentId?: string;
    expectedVersion?: number;
  },
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  const { write } = await holdingResources(actor.holdingId),
    list = await accounts(write, actor, input.chartId, []);
  let rows: Record<string, string>[];
  try {
    rows = financeCsvRows(input.csv);
  } catch (e) {
    throw new ApiError(
      422,
      "INVALID_CSV",
      String(e instanceof Error ? e.message : e),
    );
  }
  const lines = rows.map((r) => ({
    accountId: list.find((a) => a.code === r.account)?.id ?? "",
    month: r.month,
    amount: r.amount,
    dimensions: Object.fromEntries(
      Object.entries(r)
        .filter(([k, v]) => k.startsWith("dimension:") && v)
        .map(([k, v]) => [k.slice(10), v]),
    ),
  }));
  return prepareBudgetEdition(
    actor,
    budgetEditionSchema.parse({ ...input, lines }),
    key,
  );
}
function nextMonth(start: string, offset: number) {
  const [year, month] = start.split("-").map(Number),
    date = new Date(Date.UTC(year, month - 1 + offset, 1));
  return date.toISOString().slice(0, 7);
}
export async function prepareForecastEdition(
  actor: WorkContext,
  input: z.infer<typeof forecastEditionSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  input = forecastEditionSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId);
  await accounts(
    write,
    actor,
    input.chartId,
    input.lines.map((l) => l.accountId),
    input.lines.map((l) => l.dimensions),
  );
  const months = input.lines.flatMap((l) => {
    let base = money(l.monthlyAmount);
    return Array.from({ length: 12 }, (_, i) => {
      if (i) base = safe(multiplyBps(BigInt(base), 10000 + l.monthlyGrowthBps));
      return {
        accountId: l.accountId,
        month: nextMonth(input.startMonth, i),
        dimensions: l.dimensions,
        baseMinor: base,
        goodMinor: safe(multiplyBps(BigInt(base), 10000 + l.goodBps)),
        badMinor: safe(multiplyBps(BigInt(base), 10000 + l.badBps)),
      };
    });
  });
  return save(
    actor,
    "forecast",
    input,
    {
      assumptions: input.assumptions,
      inputs: input.lines,
      startMonth: input.startMonth,
      months,
    },
    key,
  );
}
export async function prepareFinancialReport(
  actor: WorkContext,
  input: z.infer<typeof financialReportSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  input = financialReportSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId);
  const groups = input.rows.filter((r) => r.kind === "accounts");
  await accounts(
    write,
    actor,
    input.chartId,
    [
      ...groups.flatMap((r) => r.accountIds),
      ...(input.cashFlow?.bankAccountIds ?? []),
      ...(input.cashFlow?.classifications.map((r) => r.accountId) ?? []),
    ],
    groups.map((r) => r.dimensions),
  );
  if (input.cashFlow)
    for (const id of input.cashFlow.bankAccountIds)
      if (
        !(await write
          .prepare(
            "SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND control_kind='bank'",
          )
          .bind(id, actor.companyId)
          .first())
      )
        throw new ApiError(
          422,
          "BANK_ACCOUNT_REQUIRED",
          "Cash flow requires bank control accounts.",
        );
  return save(
    actor,
    "report",
    input,
    {
      rows: input.rows,
      ...(input.cashFlow ? { cashFlow: input.cashFlow } : {}),
    },
    key,
  );
}
export async function decideFinancePlan(
  actor: WorkContext,
  id: string,
  input: z.infer<typeof financeReviewSchema>,
  action: "approve" | "reject",
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  input = financeReviewSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId),
    op = `finance.plan.${action}`;
  return financeTransaction(write,async write=>{
  const prior = await replayCommand(write, actor, key, op, id, input);
  if (prior) return prior;
  const row = await write
    .prepare("SELECT * FROM finance_plan_editions WHERE id=? AND company_id=?")
    .bind(id, actor.companyId)
    .first<PlanRow>();
  if (!row)
    throw new ApiError(404, "PLAN_NOT_FOUND", "Finance edition not found.");
  if (row.status !== "draft" || row.version !== input.expectedVersion)
    throw new ApiError(
      409,
      "STALE_VERSION",
      "Reload this edition before review.",
    );
  if (samePerson(actor, row.prepared_by))
    throw new ApiError(
      403,
      "SELF_REVIEW_FORBIDDEN",
      "Another person must approve this edition.",
    );
  if(action==="approve"){
   const content=JSON.parse(row.content_json) as {lines?:{accountId:string;dimensions:Record<string,string>}[];months?:{accountId:string;dimensions:Record<string,string>}[];rows?:z.infer<typeof reportRowSchema>[];cashFlow?:z.infer<typeof financialReportSchema>["cashFlow"]};
   const lines=content.lines??content.months??[],groups=(content.rows??[]).filter(r=>r.kind==="accounts");
   await accounts(write,actor,row.chart_id,[...lines.map(l=>l.accountId),...groups.flatMap(g=>g.accountIds),...(content.cashFlow?.bankAccountIds??[]),...(content.cashFlow?.classifications.map(c=>c.accountId)??[])],[...lines.map(l=>l.dimensions),...groups.map(g=>g.dimensions)]);
  }
  return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    row.version + 1,
    {
      id,
      status: action === "approve" ? "approved" : "rejected",
      version: row.version + 1,
    },
    [
      assertion(
        write,
        key,
        "version",
        "EXISTS(SELECT 1 FROM finance_plan_editions WHERE id=? AND company_id=? AND version=? AND status='draft')",
        [id, actor.companyId, input.expectedVersion],
      ),
      write
        .prepare(
          "UPDATE finance_plan_editions SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE id=?",
        )
        .bind(
          action === "approve" ? "approved" : "rejected",
          actor.userId,
          input.reason,
          Date.now(),
          id,
        ),
    ],
  );
  });
}
export async function financePlanningWorkspace(
  actor: WorkContext,
  chartId: string,
) {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const list = await accounts(write, actor, chartId, []);
  return {
    accounts: list,
    editions: (
      await write
        .prepare(
          "SELECT * FROM finance_plan_editions WHERE company_id=? AND chart_id=? ORDER BY created_at DESC,id",
        )
        .bind(actor.companyId, chartId)
        .all()
    ).results,
    snapshots:(await write.prepare("SELECT s.id,s.definition_id,s.start_date,s.end_date,s.source_hash,s.created_at,p.name FROM finance_report_snapshots s JOIN finance_plan_editions p ON p.id=s.definition_id WHERE s.company_id=? AND p.chart_id=? ORDER BY s.created_at DESC LIMIT 200").bind(actor.companyId,chartId).all()).results,
  };
}
async function plan(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  kind: string,
) {
  const row = await db
    .prepare(
      "SELECT * FROM finance_plan_editions WHERE id=? AND company_id=? AND kind=? AND status='approved'",
    )
    .bind(id, actor.companyId, kind)
    .first<PlanRow>();
  if (!row)
    throw new ApiError(
      404,
      "APPROVED_PLAN_NOT_FOUND",
      "Select an approved Company edition.",
    );
  return row;
}
export async function budgetActuals(actor: WorkContext, id: string) {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId),
    row = await plan(write, actor, id, "budget"),
    content = JSON.parse(row.content_json) as BudgetContent;
  const lines = [];
  for (const l of content.lines) {
    const dims = canonicalDimensions(l.dimensions),
      actual = Number(
        (await write
          .prepare(
            `SELECT COALESCE(SUM(jl.base_debit_minor-jl.base_credit_minor),0) value FROM finance_journal_lines jl JOIN finance_journals j ON j.id=jl.journal_id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND NOT ${fiscalCloseSourceSql()} AND jl.account_id=? AND left(j.entry_date,7)=? AND jl.dimension_json::jsonb @> ?::jsonb`,
          )
          .bind(actor.companyId, row.chart_id, l.accountId, l.month, dims)
          .first("value")) ?? 0,
      );
    const account = await write
        .prepare("SELECT category FROM finance_accounts WHERE id=?")
        .bind(l.accountId)
        .first<{ category: string }>(),
      natural = ["liability", "equity", "revenue", "other_income"].includes(
        account!.category,
      )
        ? -actual
        : actual;
    lines.push({
      ...l,
      budget: minorToDecimal(l.minor),
      actual: minorToDecimal(natural),
      variance: minorToDecimal(safe(BigInt(natural) - BigInt(l.minor))),
      actualMinor: natural,
    });
  }
  return {
    id,
    edition: row.edition,
    chartId: row.chart_id,
    year: content.year,
    lines,
  };
}
type LedgerLine = {
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
async function ledger(
  db: SqlDatabase,
  actor: WorkContext,
  chartId: string,
  endDate: string,
) {
  return (
    await db
      .prepare(
        `SELECT l.id,j.id AS "journalId",l.account_id AS "accountId",j.entry_date AS "entryDate",l.base_debit_minor AS debit,l.base_credit_minor AS credit,l.dimension_json AS dimensions,j.source_type AS "sourceType",CASE WHEN ${fiscalCloseSourceSql()} THEN 1 ELSE 0 END AS "isFiscalClose",j.source_id AS "sourceId" FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date<=? ORDER BY j.entry_date,j.id,l.ordinal`,
      )
      .bind(actor.companyId, chartId, endDate)
      .all<LedgerLine>()
  ).results.map(({ isFiscalClose, ...line }) =>
    Number(isFiscalClose) === 1 && line.sourceType === "book_projection"
      ? { ...line, isFiscalClose: 1 }
      : line,
  );
}
function reportValues(
  rows: z.infer<typeof financialReportSchema>["rows"],
  source: LedgerLine[],
  start: string,
) {
  const result: {
      key: string;
      label: string;
      value: string | null;
      minor: number | null;
      unit: string;
      journalIds: string[];
    }[] = [],
    values = new Map<string, bigint | null>();
  for (const r of rows) {
    let n: bigint | null = BigInt(0),
      ids: string[] = [];
    if (r.kind === "accounts") {
      const selected = source.filter(
        (l) =>
          r.accountIds.includes(l.accountId) &&
          (r.basis === "cumulative" ||
            (l.entryDate >= start && l.sourceType !== "fiscal_close" && Number(l.isFiscalClose) !== 1)) &&
          Object.entries(r.dimensions).every(
            ([k, v]) => JSON.parse(l.dimensions)[k] === v,
          ),
      );
      n =
        selected.reduce(
          (s, l) => s + BigInt(l.debit) - BigInt(l.credit),
          BigInt(0),
        ) * (r.sign === "debit" ? BigInt(1) : BigInt(-1));
      ids = [...new Set(selected.map((l) => l.journalId))];
    } else {
      const v = r.references.map((k) => values.get(k) ?? null);
      if (v.some((x) => x === null)) n = null;
      else if (r.kind === "sum")
        n = (v as bigint[]).reduce((s, a) => s + a, BigInt(0));
      else if (r.kind === "difference") n = v[0]! - v[1]!;
      else if(v[1]===BigInt(0))n=null;
      else {const numerator=v[0]!*BigInt(10000),denominator=v[1]!,negative=(numerator<BigInt(0))!==(denominator<BigInt(0)),absolute=numerator<BigInt(0)?-numerator:numerator,divisor=denominator<BigInt(0)?-denominator:denominator;n=((absolute+divisor/BigInt(2))/divisor)*(negative?BigInt(-1):BigInt(1));}
      ids = [
        ...new Set(
          result
            .filter((x) => r.references.includes(x.key))
            .flatMap((x) => x.journalIds),
        ),
      ];
    }
    values.set(r.key, n);
    result.push({
      key: r.key,
      label: r.label,
      value: n === null ? null : minorToDecimal(safe(n)),
      minor: n === null ? null : safe(n),
      unit: r.kind === "ratio" ? "percent" : "money",
      journalIds: ids,
    });
  }
  return result;
}
export async function snapshotFinancialReport(
  actor: WorkContext,
  id: string,
  input: { startDate: string; endDate: string },
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  input = z
    .object({ startDate: z.string().date(), endDate: z.string().date() })
    .refine((v) => v.endDate >= v.startDate)
    .parse(input);
  const { write } = await holdingResources(actor.holdingId);
  if (!isTransactionalSqlDatabase(write))
    throw new ApiError(
      503,
      "TRANSACTION_REQUIRED",
      "Report snapshots require PostgreSQL.",
    );
  return financeTransaction(write,async (db) => {
    const op = "finance.report.snapshot",
      prior = await replayCommand<{
        id: string;
        rows: ReturnType<typeof reportValues>;
      }>(db, actor, key, op, id, input);
    if (prior) return prior;
    const row = await plan(db, actor, id, "report");
    await db
      .prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))")
      .bind(`finance-book-${row.chart_id}`)
      .run();
    const source = await ledger(db, actor, row.chart_id, input.endDate),
      definition = JSON.parse(row.content_json) as Pick<
        z.infer<typeof financialReportSchema>,
        "rows" | "cashFlow"
      >,
      sourceHash = await sha256(JSON.stringify(source)),
      rows = reportValues(definition.rows, source, input.startDate),
      cashFlow = definition.cashFlow
        ? calculateCashFlow(source, input.startDate, definition.cashFlow)
        : null,
      snapshotId = crypto.randomUUID(),
      result = {
        id: snapshotId,
        definitionId: id,
        edition: row.edition,
        chartId: row.chart_id,
        ...input,
        sourceHash,
        rows,
        cashFlow,
      };
    const csv=reportCsv(result),exportHash=await sha256(csv);
    return executeCommand(db, actor, key, op, input, id, row.version, result, [
      db
        .prepare(
          "INSERT INTO finance_report_snapshots(id,company_id,definition_id,start_date,end_date,result_json,source_hash,created_by,created_at,export_content,export_sha256) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          snapshotId,
          actor.companyId,
          id,
          input.startDate,
          input.endDate,
          JSON.stringify(result),
          sourceHash,
          actor.userId,
          Date.now(),
          csv,
          exportHash,
        ),
    ]);
  });
}
function calculateCashFlow(
  source: LedgerLine[],
  start: string,
  policy: NonNullable<z.infer<typeof financialReportSchema>["cashFlow"]>,
) {
  const bank = new Set(policy.bankAccountIds),
    groups = new Map(policy.classifications.map((r) => [r.accountId, r.kind])),
    totals = {
      operating: BigInt(0),
      investing: BigInt(0),
      financing: BigInt(0),
    },
    trace: { journalId: string; kind: string; minor: number }[] = [];
  const opening = source
      .filter(
        (l) =>
          bank.has(l.accountId) &&
          (l.entryDate < start || l.sourceType === "book_opening"),
      )
      .reduce((s, l) => s + BigInt(l.debit) - BigInt(l.credit), BigInt(0)),
    closing = source
      .filter((l) => bank.has(l.accountId))
      .reduce((s, l) => s + BigInt(l.debit) - BigInt(l.credit), BigInt(0));
  const journalIds = new Set(
    source
      .filter(
        (l) =>
          bank.has(l.accountId) &&
          l.entryDate >= start &&
          l.sourceType !== "book_opening",
      )
      .map((l) => l.journalId),
  );
  for (const jid of journalIds) {
    const lines = source.filter((l) => l.journalId === jid),
      net = lines
        .filter((l) => bank.has(l.accountId))
        .reduce((s, l) => s + BigInt(l.debit) - BigInt(l.credit), BigInt(0));
    if (net === BigInt(0)) continue;
    for (const l of lines.filter((l) => !bank.has(l.accountId))) {
      const kind = groups.get(l.accountId);
      if (!kind)
        throw new ApiError(
          422,
          "CASH_FLOW_MAPPING_REQUIRED",
          `Classify the noncash account ${l.accountId} before creating the cash-flow snapshot.`,
        );
      const value = BigInt(l.credit) - BigInt(l.debit);
      totals[kind] += value;
      trace.push({ journalId: jid, kind, minor: safe(value) });
    }
  }
  const change = totals.operating + totals.investing + totals.financing;
  if (opening + change !== closing)
    throw new ApiError(
      409,
      "CASH_FLOW_RECONCILIATION",
      "Cash-flow movements do not reconcile to bank balances.",
    );
  return {
    opening: minorToDecimal(safe(opening)),
    operating: minorToDecimal(safe(totals.operating)),
    investing: minorToDecimal(safe(totals.investing)),
    financing: minorToDecimal(safe(totals.financing)),
    change: minorToDecimal(safe(change)),
    closing: minorToDecimal(safe(closing)),
    trace,
  };
}
type RetainedReport={rows:{label:string;value:string|null;unit:string}[];cashFlow?:Record<string,unknown>|null};
function reportCsv(result:RetainedReport,escapeFormulas=true){return financeCsv(["Row","Amount","Unit"],[...result.rows.map(r=>[r.label,r.value,r.unit]),...(result.cashFlow?["opening","operating","investing","financing","change","closing"].map(key=>[`Cash flow: ${key}`,result.cashFlow![key],"money"]):[])],escapeFormulas);}
export async function exportFinancialReport(actor:WorkContext,id:string){
 actor=requirePermission(actor,"finance.read");const {write}=await holdingResources(actor.holdingId),row=await write.prepare("SELECT result_json,export_content,export_sha256 FROM finance_report_snapshots WHERE id=? AND company_id=?").bind(id,actor.companyId).first<{result_json:string;export_content:string|null;export_sha256:string|null}>();
 if(!row)throw new ApiError(404,"REPORT_NOT_FOUND","Report snapshot not found.");
 const content=row.export_content??reportCsv(JSON.parse(row.result_json) as RetainedReport,false),hash=await sha256(content);
 if(row.export_sha256&&hash!==row.export_sha256)throw new ApiError(409,"REPORT_EXPORT_INTEGRITY","The retained export does not match its recorded checksum.");
 return {content,sha256:hash};
}

export async function financePlanRecord(actor:WorkContext,id:string){
 requirePermission(actor,'finance.read');const {write}=await holdingResources(actor.holdingId);
 const row=await write.prepare('SELECT * FROM finance_plan_editions WHERE id=? AND company_id=?').bind(id,actor.companyId).first<PlanRow>();
 if(!row)throw new ApiError(404,'PLAN_NOT_FOUND','Finance edition not found.');
 const data=await financePlanningWorkspace(actor,row.chart_id);
 return {row,data:{...data,editions:data.editions.filter(edition=>edition.id===id),snapshots:data.snapshots.filter(snapshot=>snapshot.definition_id===id)}};
}
export async function budgetSourceRecords(actor:WorkContext,id:string,index:number,page=1){
 const report=await budgetActuals(actor,id),line=report.lines[index];if(!line)throw new ApiError(404,'BUDGET_LINE_NOT_FOUND','Budget line not found.');
 const {write}=await holdingResources(actor.holdingId);
 const rows=(await write.prepare(`SELECT jl.*,j.id journal_id,j.journal_number,j.entry_date,j.currency,j.status FROM finance_journal_lines jl JOIN finance_journals j ON j.id=jl.journal_id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND NOT ${fiscalCloseSourceSql()} AND jl.account_id=? AND left(j.entry_date,7)=? AND jl.dimension_json::jsonb @> ?::jsonb ORDER BY j.entry_date,j.id,jl.ordinal LIMIT 31 OFFSET ?`).bind(actor.companyId,report.chartId,line.accountId,line.month,canonicalDimensions(line.dimensions),(page-1)*30).all()).results;
 return {row:{name:`Budget actuals · ${line.month}`,reported_total:line.actual,description:`Budget ${line.budget} · Variance ${line.variance}`},rows,page,hasNext:rows.length>30};
}
export async function financialReportSourceRecords(actor:WorkContext,id:string,args:Record<string,string>,page=1){
 requirePermission(actor,'finance.read');const {write}=await holdingResources(actor.holdingId);
 const saved=await write.prepare('SELECT s.*,p.content_json,p.chart_id FROM finance_report_snapshots s JOIN finance_plan_editions p ON p.id=s.definition_id WHERE s.id=? AND s.company_id=? AND p.company_id=?').bind(id,actor.companyId,actor.companyId).first<{result_json:string;content_json:string;source_hash:string;chart_id:string;start_date:string;end_date:string}>();
 if(!saved)throw new ApiError(404,'REPORT_NOT_FOUND','Report snapshot not found.');
 if(!args.key&&!args.cashFlow)return {row:{name:'Retained financial report',start_date:saved.start_date,end_date:saved.end_date,result:JSON.parse(saved.result_json),source_hash:saved.source_hash},rows:[],page,hasNext:false};
 const source=await ledger(write,actor,saved.chart_id,saved.end_date);
 if(await sha256(JSON.stringify(source))!==saved.source_hash)throw new ApiError(409,'REPORT_SOURCES_CHANGED','The ledger changed since this snapshot. Open the retained report or create a current report.');
 const result=JSON.parse(saved.result_json) as {rows:ReturnType<typeof reportValues>;cashFlow:ReturnType<typeof calculateCashFlow>|null},definition=JSON.parse(saved.content_json) as Pick<z.infer<typeof financialReportSchema>,'rows'|'cashFlow'>;
 let ids:string[]=[],label='',value:string|null=null;
 if(args.cashFlow){
  const key=z.enum(['opening','operating','investing','financing','change','closing']).parse(args.cashFlow),cash=result.cashFlow;
  if(!cash||!definition.cashFlow)throw new ApiError(404,'CASH_FLOW_NOT_FOUND','Cash-flow sources unavailable.');
  label=`Cash flow · ${key}`;value=cash[key];
  ids=['opening','closing'].includes(key)?source.filter(line=>definition.cashFlow!.bankAccountIds.includes(line.accountId)&&(key==='closing'||line.entryDate<saved.start_date||line.sourceType==='book_opening')).map(line=>line.journalId):cash.trace.filter(item=>key==='change'||item.kind===key).map(item=>item.journalId);
 }else{const row=result.rows.find(row=>row.key===args.key);if(!row)throw new ApiError(404,'REPORT_ROW_NOT_FOUND','Report row not found.');ids=row.journalIds;label=row.label;value=row.value;}
 ids=[...new Set(ids)];const selected=ids.slice((page-1)*30,page*30);
 const rows=selected.length?(await write.prepare(`SELECT id journal_id,journal_number,entry_date,currency,status,(SELECT SUM(l.base_debit_minor) FROM finance_journal_lines l WHERE l.journal_id=finance_journals.id) base_debit_minor,(SELECT SUM(l.base_credit_minor) FROM finance_journal_lines l WHERE l.journal_id=finance_journals.id) base_credit_minor FROM finance_journals WHERE company_id=? AND id IN (${selected.map(()=>'?').join(',')}) ORDER BY entry_date,id`).bind(actor.companyId,...selected).all()).results:[];
 return {row:{name:label,reported_total:value,start_date:saved.start_date,end_date:saved.end_date},rows,page,hasNext:ids.length>page*30};
}
