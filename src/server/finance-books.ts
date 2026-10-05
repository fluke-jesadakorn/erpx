import { financeTransaction } from "./finance-transactions";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { isTransactionalSqlDatabase } from "./runtime/types";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { samePerson } from "@/lib/iam";
import { sha256 } from "./crypto";
import { requireFinanceMoneyScale } from "./finance-money-policy";

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
export const financeReviewSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});
type OpeningRow = { accountId: string; dimensions: string; minor: number };
const openingSql = `SELECT l.account_id AS "accountId",l.dimension_json AS dimensions,SUM(l.base_debit_minor-l.base_credit_minor) AS minor
 FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
 WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date<?
 GROUP BY l.account_id,l.dimension_json HAVING SUM(l.base_debit_minor-l.base_credit_minor)<>0 ORDER BY l.account_id,l.dimension_json`;
async function opening(
  db: SqlDatabase,
  company: string,
  chart: string,
  date: string,
) {
  return (
    await db.prepare(openingSql).bind(company, chart, date).all<OpeningRow>()
  ).results;
}
async function commandReplay(
  db: SqlDatabase,
  actor: WorkContext,
  key: string,
  op: string,
  input: unknown,
) {
  const receipt = await db
    .prepare("SELECT record_id FROM command_receipts WHERE command_id=?")
    .bind(key)
    .first<{ record_id: string }>();
  return receipt
    ? replayCommand<Record<string, unknown>>(
        db,
        actor,
        key,
        op,
        receipt.record_id,
        input,
      )
    : null;
}
export async function financeBookWorkspace(actor: WorkContext) {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const [definitions, values, rules, mappings, charts, accounts, periods] =
    await Promise.all([
      write
        .prepare(
          "SELECT * FROM finance_dimension_definitions WHERE company_id=? ORDER BY code",
        )
        .bind(actor.companyId)
        .all(),
      write
        .prepare(
          "SELECT * FROM finance_dimension_values WHERE company_id=? ORDER BY dimension_id,code",
        )
        .bind(actor.companyId)
        .all(),
      write
        .prepare(
          "SELECT r.*,p.name primary_name,s.name secondary_name FROM finance_book_rules r JOIN finance_charts p ON p.id=r.primary_chart_id JOIN finance_charts s ON s.id=r.secondary_chart_id WHERE r.company_id=? ORDER BY r.effective_date DESC,r.created_at DESC",
        )
        .bind(actor.companyId)
        .all(),
      write
        .prepare(
          "SELECT m.* FROM finance_book_account_mappings m JOIN finance_book_rules r ON r.id=m.rule_id WHERE r.company_id=? ORDER BY m.rule_id,m.primary_account_id",
        )
        .bind(actor.companyId)
        .all(),
      write
        .prepare(
          "SELECT id,name,base_currency FROM finance_charts WHERE company_id=? AND status='active' ORDER BY name",
        )
        .bind(actor.companyId)
        .all(),
      write
        .prepare(
          "SELECT id,chart_id,code,name FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY chart_id,code",
        )
        .bind(actor.companyId)
        .all(),
      write
        .prepare(
          "SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC",
        )
        .bind(actor.companyId)
        .all(),
    ]);
  return {
    definitions: definitions.results,
    values: values.results,
    rules: rules.results,
    mappings: mappings.results,
    charts: charts.results,
    accounts: accounts.results,
    periods: periods.results,
  };
}
export async function createFinanceDimension(
  actor: WorkContext,
  input: z.infer<typeof dimensionSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  input = dimensionSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId),
    op = "finance.dimension.create";
  const prior = await commandReplay(write, actor, key, op, input);
  if (prior) return prior;
  const id = crypto.randomUUID(),
    now = Date.now();
  return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    1,
    { id, status: "active", version: 1 },
    [
      write
        .prepare(
          "INSERT INTO finance_dimension_definitions(id,company_id,code,name,required,created_by,created_at) VALUES(?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          input.code,
          input.name,
          input.required ? 1 : 0,
          actor.userId,
          now,
        ),
      ...input.values.map((v) =>
        write
          .prepare(
            "INSERT INTO finance_dimension_values(id,dimension_id,company_id,code,name,created_at) VALUES(?,?,?,?,?,?)",
          )
          .bind(crypto.randomUUID(), id, actor.companyId, v.code, v.name, now),
      ),
    ],
  );
}
export async function prepareBookRule(
  actor: WorkContext,
  input: z.infer<typeof bookRuleSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  input = bookRuleSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId),
    op = "finance.book.prepare";
  const prior = await commandReplay(write, actor, key, op, input);
  if (prior) return prior;
  const charts = (
    await write
      .prepare(
        "SELECT * FROM finance_charts WHERE company_id=? AND id IN (?,?) AND status='active'",
      )
      .bind(actor.companyId, input.primaryChartId, input.secondaryChartId)
      .all<{ id: string; base_currency: string }>()
  ).results;
  if (charts.length !== 2)
    throw new ApiError(
      422,
      "BOOK_CHART_REQUIRED",
      "Select two active Company books.",
    );
  await requireFinanceMoneyScale(write,actor.companyId,charts[0].base_currency);
  await requireFinanceMoneyScale(write,actor.companyId,charts[1].base_currency);
  if (charts[0].base_currency !== charts[1].base_currency)
    throw new ApiError(
      422,
      "BOOK_CURRENCY_ADAPTER_REQUIRED",
      "The initial book adapter requires the same functional currency to preserve settled carrying amounts.",
    );
  if (
    await write
      .prepare(
        "SELECT 1 FROM finance_book_rules WHERE company_id=? AND status='approved' AND (secondary_chart_id=? OR primary_chart_id=?)",
      )
      .bind(actor.companyId, input.primaryChartId, input.secondaryChartId)
      .first()
  )
    throw new ApiError(
      422,
      "BOOK_CHAIN_FORBIDDEN",
      "A secondary book cannot be another book's source.",
    );
  const existingRule = await write
    .prepare(
      "SELECT primary_chart_id FROM finance_book_rules WHERE secondary_chart_id=? AND status='approved' ORDER BY effective_date DESC LIMIT 1",
    )
    .bind(input.secondaryChartId)
    .first<{ primary_chart_id: string }>();
  if (existingRule && existingRule.primary_chart_id !== input.primaryChartId)
    throw new ApiError(
      422,
      "BOOK_SOURCE_IMMUTABLE",
      "An active secondary book keeps its original primary book.",
    );
  if (
    !existingRule &&
    (await write
      .prepare(
        "SELECT 1 FROM finance_journals WHERE chart_id=? AND status IN ('posted','reversed')",
      )
      .bind(input.secondaryChartId)
      .first())
  )
    throw new ApiError(
      422,
      "BOOK_TARGET_NOT_EMPTY",
      "Start the secondary book with a reviewed opening; existing posted ledgers cannot be attached.",
    );
  const period = await write
    .prepare(
      "SELECT 1 FROM finance_periods WHERE company_id=? AND id=? AND state='open' AND start_date<=? AND end_date>=?",
    )
    .bind(
      actor.companyId,
      input.openingPeriodId,
      input.effectiveDate,
      input.effectiveDate,
    )
    .first();
  if (!period)
    throw new ApiError(
      422,
      "BOOK_OPENING_PERIOD_REQUIRED",
      "The opening date must be in an open Company period.",
    );
  const accounts = (
    await write
      .prepare(
        "SELECT id,chart_id,category,control_kind,currency,status FROM finance_accounts WHERE company_id=? AND chart_id IN (?,?)",
      )
      .bind(actor.companyId, input.primaryChartId, input.secondaryChartId)
      .all<{
        id: string;
        chart_id: string;
        category: string;
        control_kind: string;
        currency: string | null;
        status: string;
      }>()
  ).results;
  for (const m of input.mappings) {
    const src = accounts.find((a) => a.id === m.primaryAccountId),
      dst = accounts.find((a) => a.id === m.secondaryAccountId);
    if (
      !src ||
      !dst ||
      src.chart_id !== input.primaryChartId ||
      dst.chart_id !== input.secondaryChartId ||
      src.status !== "active" ||
      dst.status !== "active" ||
      src.category !== dst.category ||
      src.control_kind !== dst.control_kind ||
      src.currency !== dst.currency
    )
      throw new ApiError(
        422,
        "BOOK_ACCOUNT_MAPPING_INVALID",
        "Map active Company accounts with the same category, currency and control kind.",
      );
    if (
      existingRule &&
      (await write
        .prepare(
          "SELECT 1 FROM finance_book_account_mappings m JOIN finance_book_rules r ON r.id=m.rule_id WHERE r.secondary_chart_id=? AND r.status='approved' AND m.primary_account_id=? AND m.secondary_account_id<>?",
        )
        .bind(input.secondaryChartId, m.primaryAccountId, m.secondaryAccountId)
        .first())
    )
      throw new ApiError(
        422,
        "BOOK_CARRYING_MAPPING_CHANGED",
        "Keep existing account mappings; moving carrying balances requires a reviewed book transition.",
      );
  }
  if (
    accounts.some(
      (a) =>
        a.chart_id === input.primaryChartId &&
        a.status === "active" &&
        !input.mappings.some((m) => m.primaryAccountId === a.id),
    )
  )
    throw new ApiError(
      422,
      "BOOK_ACCOUNT_MAPPING_REQUIRED",
      "Map every active source account before enabling the book.",
    );
  const rows = await opening(
    write,
    actor.companyId,
    input.primaryChartId,
    input.effectiveDate,
  );
  if (
    rows.some((r) => !Number.isSafeInteger(r.minor)) ||
    rows.reduce((s, r) => s + BigInt(r.minor), BigInt(0)) !== BigInt(0)
  )
    throw new ApiError(
      422,
      "BOOK_OPENING_UNBALANCED",
      "The source opening must balance within supported money precision.",
    );
  const id = crypto.randomUUID(),
    now = Date.now(),
    openingHash = await sha256(JSON.stringify(rows));
  return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    1,
    { id, status: "draft", version: 1, openingHash, opening: rows },
    [
      write
        .prepare(
          "INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,prepared_by,reason,opening_json,opening_hash,opening_period_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          input.primaryChartId,
          input.secondaryChartId,
          input.effectiveDate,
          actor.userId,
          input.reason,
          JSON.stringify(rows),
          openingHash,
          input.openingPeriodId,
          now,
        ),
      ...input.mappings.map((m) =>
        write
          .prepare(
            "INSERT INTO finance_book_account_mappings(rule_id,primary_account_id,secondary_account_id) VALUES(?,?,?)",
          )
          .bind(id, m.primaryAccountId, m.secondaryAccountId),
      ),
    ],
  );
}
export async function decideBookRule(
  actor: WorkContext,
  id: string,
  input: z.infer<typeof financeReviewSchema>,
  action: "approve" | "reject",
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  input = financeReviewSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId);
  if (!isTransactionalSqlDatabase(write))
    throw new ApiError(
      503,
      "TRANSACTION_REQUIRED",
      "Book approval requires PostgreSQL transactions.",
    );
  return financeTransaction(write,async (db) => {
    const op = `finance.book.${action}`,
      prior = await replayCommand<Record<string, unknown>>(
        db,
        actor,
        key,
        op,
        id,
        input,
      );
    if (prior) return prior;
    const rule = await db
      .prepare(
        "SELECT * FROM finance_book_rules WHERE id=? AND company_id=? FOR UPDATE",
      )
      .bind(id, actor.companyId)
      .first<{
        status: string;
        version: number;
        primary_chart_id: string;
        secondary_chart_id: string;
        effective_date: string;
        opening_json: string;
        opening_hash: string;
        opening_period_id: string;
        prepared_by: string;
      }>();
    if (!rule)
      throw new ApiError(404, "BOOK_RULE_NOT_FOUND", "Book rule not found.");
    if (rule.status !== "draft" || rule.version !== input.expectedVersion)
      throw new ApiError(
        409,
        "STALE_VERSION",
        "Reload this book rule before review.",
      );
    if (samePerson(actor, rule.prepared_by))
      throw new ApiError(
        403,
        "SELF_REVIEW_FORBIDDEN",
        "Another person must approve the book rule and opening.",
      );
    await db
      .prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))")
      .bind(`finance-book-${rule.primary_chart_id}`)
      .run();
    const now = Date.now(),
      rows = JSON.parse(rule.opening_json) as OpeningRow[],
      steps = [];
    if (action === "approve") {
      if (
        (await sha256(
          JSON.stringify(
            await opening(
              db,
              actor.companyId,
              rule.primary_chart_id,
              rule.effective_date,
            ),
          ),
        )) !== rule.opening_hash
      )
        throw new ApiError(
          409,
          "STALE_BOOK_OPENING",
          "Source balances changed. Prepare a fresh opening for review.",
        );
      if (
        await db
          .prepare(
            "SELECT 1 FROM finance_journals WHERE chart_id=? AND status IN ('posted','reversed') AND entry_date>=?",
          )
          .bind(rule.primary_chart_id, rule.effective_date)
          .first()
      )
        throw new ApiError(
          409,
          "BOOK_ACTIVATION_DATE_USED",
          "Choose an activation date after all existing source postings.",
        );
      const alreadyActive = await db
        .prepare(
          "SELECT 1 FROM finance_book_rules WHERE secondary_chart_id=? AND status='approved'",
        )
        .bind(rule.secondary_chart_id)
        .first();
      if (
        !alreadyActive &&
        (await db
          .prepare(
            "SELECT 1 FROM finance_journals WHERE chart_id=? AND status IN ('posted','reversed')",
          )
          .bind(rule.secondary_chart_id)
          .first())
      )
        throw new ApiError(
          409,
          "BOOK_TARGET_NOT_EMPTY",
          "The target ledger changed. Prepare a new rule.",
        );
      steps.push(
        assertion(
          db,
          key,
          "opening-period",
          "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)",
          [
            rule.opening_period_id,
            actor.companyId,
            rule.effective_date,
            rule.effective_date,
          ],
        ),
      );
      if (rows.length && !alreadyActive) {
        const jid = `book-opening-${id}`;
        steps.push(
          db
            .prepare(
              "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,is_opening,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,id,?,?,?,base_currency,1000000,'book_opening',?,'Reviewed secondary book opening','posted',1,?,?,?,?,? FROM finance_charts WHERE id=?",
            )
            .bind(
              jid,
              rule.opening_period_id,
              jid,
              rule.effective_date,
              id,
              rule.prepared_by,
              actor.userId,
              now,
              now,
              now,
              rule.secondary_chart_id,
            ),
        );
        rows.forEach((r, i) =>
          steps.push(
            db
              .prepare(
                "INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?,?,secondary_account_id,?,?,?,?,?,? FROM finance_book_account_mappings WHERE rule_id=? AND primary_account_id=?",
              )
              .bind(
                `${jid}-${i}`,
                jid,
                Math.max(r.minor, 0),
                Math.max(-r.minor, 0),
                Math.max(r.minor, 0),
                Math.max(-r.minor, 0),
                r.dimensions,
                i,
                id,
                r.accountId,
              ),
          ),
        );
      }
    }
    steps.push(
      db
        .prepare(
          "UPDATE finance_book_rules SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE id=?",
        )
        .bind(
          action === "approve" ? "approved" : "rejected",
          actor.userId,
          input.reason,
          now,
          id,
        ),
    );
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      rule.version + 1,
      {
        id,
        status: action === "approve" ? "approved" : "rejected",
        version: rule.version + 1,
      },
      steps,
    );
  });
}

export async function financeBookRecord(actor:WorkContext,id:string){
 const data=await financeBookWorkspace(actor),row=data.rules.find(rule=>rule.id===id);
 if(!row)throw new ApiError(404,'BOOK_RULE_NOT_FOUND','Book rule not found in this Company.');
 return {row,data:{...data,rules:[row],mappings:data.mappings.filter(mapping=>mapping.rule_id===id)}};
}
