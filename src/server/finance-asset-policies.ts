import "server-only";
import { z } from "zod";
import {
  depreciationPolicySchema,
  depreciationAmount,
  monthlyDate,
} from "@/lib/finance-instrument-policy";
import { decimalToUnits, unitsToDecimal } from "@/lib/finance-money";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "./authorization";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { executeCommand, replayCommand, assertion } from "./commands";
import {
  financeTransaction,
  financeOwnedTransaction,
} from "./finance-transactions";
import { requireFinanceTool } from "./finance-company-tools";
import { prepareJournal, type JournalLineInput } from "./finance";
import { prepareOwnedJournalReversal } from "./finance-owned-reversal";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
import {
  assetCarryingEvidence,
  type AssetCarryBasis,
} from "./finance-asset-carrying";
import {
  instrumentDecisionSchema,
  instrumentReversalSchema,
} from "./finance-instruments";
export const assetPolicySchema = z.object({
  assetId: z.string().min(1),
  expectedVersion: z.number().int().positive(),
  parentId: z.string().min(1).optional(),
  parentVersion: z.number().int().positive().optional(),
  effectiveDate: z.string().date().optional(),
  adoptLegacy: z.boolean().optional(),
  books: z
    .array(
      z.object({
        chartId: z.string().min(1),
        policy: depreciationPolicySchema,
      }),
    )
    .min(1)
    .max(20),
  dimensions: z.record(z.string(), z.string()).default({}),
  reason: z.string().trim().min(3).max(2000),
});
export const assetPolicyRunSchema = z.object({
  expectedVersion: z.number().int().positive(),
  periodId: z.string().min(1),
  entryDate: z.string().date(),
  depreciationMonth: z
    .string()
    .regex(/^\d{4}-(0[1-9]|1[0-2])$/)
    .optional(),
  units: z.number().int().nonnegative().max(1000000000).default(0),
  usageEvidence: z.string().trim().min(3).max(2000).optional(),
  reason: z.string().trim().min(3).max(2000),
});
export const assetPolicyDecisionSchema = instrumentDecisionSchema;
export const assetPolicyReversalSchema = instrumentReversalSchema;
type Row = Record<string, string | number | null>;
type Book = {
  chartId: string;
  costAccount: string;
  accumulatedAccount: string;
  expenseAccount: string;
  ruleId: string | null;
};
type Result = {
  ordinal: number;
  units: number;
  books: Array<{
    chartId: string;
    amount: string;
    accumulated: string;
    units: number;
  }>;
};
async function asset(db: SqlDatabase, actor: WorkContext, id: string) {
  const row = await db
    .prepare(
      "SELECT a.*,c.chart_id FROM finance_assets a JOIN finance_accounts c ON c.id=a.account_id WHERE a.id=? AND a.company_id=?",
    )
    .bind(id, actor.companyId)
    .first<Row>();
  if (!row) throw new ApiError(404, "ASSET_NOT_FOUND", "Asset not found.");
  if (
    row.status !== "in_service" ||
    !row.activation_journal_id ||
    !row.depreciation_account_id ||
    !row.depreciation_expense_account_id
  )
    throw new ApiError(
      409,
      "ASSET_NOT_RECOGNIZED",
      "Recognize the asset with complete depreciation mappings first.",
    );
  if (
    await db
      .prepare(
        "SELECT 1 FROM finance_asset_disposals WHERE asset_id=? AND status='draft'",
      )
      .bind(id)
      .first()
  )
    throw new ApiError(
      409,
      "ASSET_DISPOSAL_PENDING",
      "Decide the asset disposal before preparing more depreciation.",
    );
  return row;
}
async function bookEvidence(
  db: SqlDatabase,
  actor: WorkContext,
  a: Row,
  definition: z.infer<typeof assetPolicySchema>,
  date: string,
) {
  const rules = (
    await db
      .prepare(
        "SELECT DISTINCT ON (secondary_chart_id) id,secondary_chart_id,effective_date FROM finance_book_rules WHERE company_id=? AND primary_chart_id=? AND status='approved' AND effective_date<=? ORDER BY secondary_chart_id,effective_date DESC",
      )
      .bind(actor.companyId, a.chart_id, date)
      .all<Row>()
  ).results;
  const ids = [
    String(a.chart_id),
    ...rules.map((r) => String(r.secondary_chart_id)),
  ];
  if (
    definition.books.length !== ids.length ||
    new Set(definition.books.map((b) => b.chartId)).size !== ids.length ||
    ids.some((id) => !definition.books.some((b) => b.chartId === id))
  )
    throw new ApiError(
      409,
      "ASSET_ALL_BOOK_POLICIES_REQUIRED",
      "Review a depreciation policy for the primary book and every enabled secondary book.",
    );
  const books: Book[] = [
    {
      chartId: String(a.chart_id),
      costAccount: String(a.account_id),
      accumulatedAccount: String(a.depreciation_account_id),
      expenseAccount: String(a.depreciation_expense_account_id),
      ruleId: null,
    },
  ];
  for (const r of rules) {
    const mappings = (
        await db
          .prepare(
            "SELECT primary_account_id,secondary_account_id FROM finance_book_account_mappings WHERE rule_id=? AND primary_account_id IN (?,?,?) ORDER BY primary_account_id",
          )
          .bind(
            r.id,
            a.account_id,
            a.depreciation_account_id,
            a.depreciation_expense_account_id,
          )
          .all<Row>()
      ).results,
      byId = new Map(
        mappings.map((m) => [
          m.primary_account_id,
          String(m.secondary_account_id),
        ]),
      );
    if (mappings.length !== 3)
      throw new ApiError(
        409,
        "ASSET_BOOK_MAPPING_REQUIRED",
        "Complete every book's asset mappings.",
      );
    books.push({
      chartId: String(r.secondary_chart_id),
      costAccount: byId.get(a.account_id!)!,
      accumulatedAccount: byId.get(a.depreciation_account_id!)!,
      expenseAccount: byId.get(a.depreciation_expense_account_id!)!,
      ruleId: String(r.id),
    });
  }
  const accountIds = books.flatMap((b) => [
      b.costAccount,
      b.accumulatedAccount,
      b.expenseAccount,
    ]),
    accounts = (
      await db
        .prepare(
          `SELECT a.id,a.version,a.category,a.control_kind,a.currency,c.status chart_status,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.status='active' AND a.id IN (${accountIds.map(() => "?").join(",")}) ORDER BY a.id`,
        )
        .bind(actor.companyId, ...accountIds)
        .all<Row>()
    ).results,
    byId = new Map(accounts.map((r) => [r.id, r]));
  if (
    accounts.length !== accountIds.length ||
    accounts.some(
      (r) =>
        r.currency !== a.currency ||
        r.base_currency !== a.currency ||
        r.chart_status !== "active" ||
        r.control_kind !== "none",
    ) ||
    books.some(
      (b) =>
        byId.get(b.costAccount)?.category !== "asset" ||
        byId.get(b.accumulatedAccount)?.category !== "asset" ||
        byId.get(b.expenseAccount)?.category !== "expense",
    )
  )
    throw new ApiError(
      409,
      "ASSET_BOOK_ACCOUNT_CHANGED",
      "Use active same-currency noncontrol asset and expense accounts in every book.",
    );
  return { books, rules, accounts };
}
async function edition(db: SqlDatabase, actor: WorkContext, id: string) {
  const row = await db
    .prepare(
      "SELECT * FROM finance_asset_policy_editions WHERE id=? AND company_id=?",
    )
    .bind(id, actor.companyId)
    .first<Row>();
  if (!row)
    throw new ApiError(
      404,
      "ASSET_POLICY_NOT_FOUND",
      "Asset depreciation policy not found.",
    );
  return row;
}
async function postedHistory(db: SqlDatabase, a: Row) {
  const legacyCount = Number(
    (await db
      .prepare(
        "SELECT count(*) n FROM finance_asset_depreciations WHERE asset_id=?",
      )
      .bind(a.id)
      .first("n")) ?? 0,
  );
  const rows = (
    await db
      .prepare(
        "SELECT * FROM finance_asset_policy_runs WHERE asset_id=? AND status='posted' ORDER BY ordinal",
      )
      .bind(a.id)
      .all<Row>()
  ).results;
  if (rows.some((r, i) => r.ordinal !== legacyCount + i + 1))
    throw new ApiError(
      409,
      "ASSET_POLICY_HISTORY_CHANGED",
      "Reconcile the posted depreciation sequence first.",
    );
  return rows;
}
async function policyEvidence(
  db: SqlDatabase,
  actor: WorkContext,
  input: z.infer<typeof assetPolicySchema>,
) {
  const a = await asset(db, actor, input.assetId);
  if (a.version !== input.expectedVersion)
    throw new ApiError(
      409,
      "ASSET_CHANGED",
      "Refresh the asset before preparing its policies.",
    );
  const history = await postedHistory(db, a),
    legacyCount = Number(
      (await db
        .prepare(
          "SELECT count(*) n FROM finance_asset_depreciations WHERE asset_id=?",
        )
        .bind(a.id)
        .first("n")) ?? 0,
    ),
    ordinal = legacyCount + history.length,
    date =
      input.effectiveDate ?? monthlyDate(String(a.in_service_date), ordinal),
    book = await bookEvidence(db, actor, a, input, date),
    parent = input.parentId ? await edition(db, actor, input.parentId) : null;
  if (
    parent &&
    (parent.status !== "active" ||
      parent.version !== input.parentVersion ||
      parent.asset_id !== a.id)
  )
    throw new ApiError(
      409,
      "ASSET_POLICY_CHANGED",
      "Amend the active policy using its current version.",
    );
  if (
    !parent &&
    (history.length ||
      ((legacyCount > 0 || Number(a.accumulated_depreciation_minor) !== 0) &&
        !input.adoptLegacy))
  )
    throw new ApiError(
      409,
      "ASSET_POLICY_OPENING_REQUIRED",
      "Choose adoption of retained asset history and review its carrying basis before changing depreciation.",
    );
  if (
    await db
      .prepare(
        "SELECT 1 FROM finance_asset_policy_runs WHERE asset_id=? AND status='draft'",
      )
      .bind(a.id)
      .first()
  )
    throw new ApiError(
      409,
      "ASSET_POLICY_RUN_PENDING",
      "Decide current depreciation drafts before revising policy.",
    );
  if (
    date.slice(0, 7) !==
      monthlyDate(String(a.in_service_date), ordinal).slice(0, 7) ||
    date < String(a.in_service_date)
  )
    throw new ApiError(
      422,
      "ASSET_POLICY_EFFECTIVE_DATE",
      "Apply the policy in the next unposted in-service month.",
    );
  const carrying = await assetCarryingEvidence(db, a, book.books);
  if (ordinal > 0 && date < carrying.basis.throughDate)
    throw new ApiError(
      422,
      "ASSET_POLICY_EFFECTIVE_DATE",
      "Apply the revised policy after the retained depreciation history.",
    );
  for (const b of input.books)
    if (
      b.policy.method === "units_of_production" &&
      b.policy.totalUnits <=
        (carrying.basis.books.find((c) => c.chartId === b.chartId)?.units ?? 0)
    )
      throw new ApiError(
        422,
        "ASSET_POLICY_USAGE_BASIS",
        "Reviewed lifetime units must include remaining usage beyond retained readings.",
      );
  return {
    a,
    parent,
    basis: carrying.basis,
    hash: await sha256(
      JSON.stringify({ input, a, book, history, parent, carrying }),
    ),
  };
}
export async function prepareAssetPolicy(
  actor: WorkContext,
  raw: z.input<typeof assetPolicySchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  const input = assetPolicySchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    id = `asset-policy-${key}`,
    op = "finance.asset-policy.prepare";
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    await requireFinanceTool(db, actor.companyId, "asset_policies");
    const e = await policyEvidence(db, actor, input),
      now = Date.now();
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      1,
      { id, status: "draft", version: 1 },
      [
        db
          .prepare(
            "INSERT INTO finance_asset_policy_editions(id,company_id,asset_id,parent_id,content_json,source_hash,prepared_by,reason,created_at,updated_at,basis_json) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            actor.companyId,
            input.assetId,
            e.parent?.id ?? null,
            JSON.stringify(input),
            e.hash,
            actor.userId,
            input.reason,
            now,
            now,
            JSON.stringify(e.basis),
          ),
      ],
    );
  });
}
export async function decideAssetPolicy(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof assetPolicyDecisionSchema>,
  action: "approve" | "reject",
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  const input = assetPolicyDecisionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = `finance.asset-policy.${action}`;
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const row = await edition(db, actor, id);
    if (row.status !== "draft" || row.version !== input.expectedVersion)
      throw new ApiError(
        409,
        "ASSET_POLICY_CHANGED",
        "Refresh the policy draft before deciding.",
      );
    if (samePerson(actor, row.prepared_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must review depreciation policy.",
      );
    const e =
      action === "approve"
        ? await policyEvidence(
            db,
            actor,
            assetPolicySchema.parse(JSON.parse(String(row.content_json))),
          )
        : null;
    if (e && e.hash !== row.source_hash)
      throw new ApiError(
        409,
        "ASSET_POLICY_SOURCE_CHANGED",
        "Asset or book evidence changed. Reject and prepare again.",
      );
    const steps: SqlStatement[] = e?.parent
      ? [
          db
            .prepare(
              "UPDATE finance_asset_policy_editions SET status='superseded',version=version+1,updated_at=? WHERE id=?",
            )
            .bind(Date.now(), e.parent.id),
        ]
      : [];
    steps.push(
      assertion(
        db,
        key,
        "asset-policy-version",
        "EXISTS(SELECT 1 FROM finance_asset_policy_editions WHERE id=? AND company_id=? AND version=? AND status='draft')",
        [id, actor.companyId, input.expectedVersion],
      ),
      db
        .prepare(
          "UPDATE finance_asset_policy_editions SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          action === "approve" ? "active" : "rejected",
          actor.userId,
          Date.now(),
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
      input.expectedVersion + 1,
      {
        id,
        status: action === "approve" ? "active" : "rejected",
        version: input.expectedVersion + 1,
      },
      steps,
    );
  });
}
async function runEvidence(
  db: SqlDatabase,
  actor: WorkContext,
  p: Row,
  input: z.infer<typeof assetPolicyRunSchema>,
) {
  if (
    !(await db
      .prepare(
        "SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?",
      )
      .bind(input.periodId, actor.companyId, input.entryDate, input.entryDate)
      .first())
  )
    throw new ApiError(
      409,
      "PERIOD_NOT_OPEN",
      "Use an open period covering the accounting date, including a zero-usage run.",
    );

  if (p.status !== "active" || p.version !== input.expectedVersion)
    throw new ApiError(
      409,
      "ASSET_POLICY_CHANGED",
      "Use the current active policy version.",
    );
  const definition = assetPolicySchema.parse(
      JSON.parse(String(p.content_json)),
    ),
    a = await asset(db, actor, String(p.asset_id)),
    history = await postedHistory(db, a),
    book = await bookEvidence(db, actor, a, definition, input.entryDate),
    basis = p.basis_json
      ? (JSON.parse(String(p.basis_json)) as AssetCarryBasis)
      : {
          ordinal: 0,
          throughDate: String(a.in_service_date),
          books: definition.books.map((b) => ({
            chartId: b.chartId,
            accumulated: "0.00",
            units: 0,
          })),
        },
    legacyCount = Number(
      (await db
        .prepare(
          "SELECT count(*) n FROM finance_asset_depreciations WHERE asset_id=?",
        )
        .bind(a.id)
        .first("n")) ?? 0,
    ),
    ordinal = legacyCount + history.length + 1,
    carrying = await assetCarryingEvidence(db, a, book.books);
  const month = (d: string) =>
    Number(d.slice(0, 4)) * 12 + Number(d.slice(5, 7));
  if (
    input.entryDate < String(a.in_service_date) ||
    month(
      input.depreciationMonth
        ? `${input.depreciationMonth}-01`
        : input.entryDate,
    ) !==
      month(String(a.in_service_date)) + ordinal - 1 ||
    (input.depreciationMonth &&
      input.entryDate.slice(0, 7) < input.depreciationMonth) ||
    input.entryDate <
      (definition.effectiveDate ??
        monthlyDate(String(a.in_service_date), basis.ordinal)) ||
    input.entryDate < carrying.latestAccountingDate
  )
    throw new ApiError(
      422,
      "ASSET_DEPRECIATION_SEQUENCE",
      "Review each asset service month in order, using a current open posting date after retained source and reversal dates.",
    );
  if (
    definition.books.some((b) => b.policy.method === "units_of_production") &&
    !input.usageEvidence
  )
    throw new ApiError(
      422,
      "ASSET_USAGE_EVIDENCE_REQUIRED",
      "Retain reviewed readings for units-of-production depreciation, including zero usage.",
    );
  const books = definition.books.map((b) => {
    const previous =
      (history.at(-1) && Number(history.at(-1)!.ordinal) > basis.ordinal
        ? (
            JSON.parse(String(history.at(-1)!.result_json)) as Result
          ).books.find((r) => r.chartId === b.chartId)
        : null) ?? basis.books.find((c) => c.chartId === b.chartId);
    if (history.length && !previous)
      throw new ApiError(
        409,
        "ASSET_BOOK_CARRYING_REQUIRED",
        "A new secondary book needs a reviewed asset carrying opening.",
      );
    const accumulated = decimalToUnits(previous?.accumulated ?? "0.00"),
      units = previous?.units ?? 0,
      baseline = basis.books.find((c) => c.chartId === b.chartId);
    if (
      carrying.basis.books.find((c) => c.chartId === b.chartId)?.accumulated !==
      unitsToDecimal(accumulated)
    )
      throw new ApiError(
        409,
        "ASSET_BOOK_CARRYING_CHANGED",
        "Book adjustments changed the asset carrying value. Review a new policy basis before depreciation.",
      );
    const amount = depreciationAmount({
      cost: BigInt(a.cost_minor!),
      residual: BigInt(a.residual_minor!),
      life: Number(a.useful_life_months),
      ordinal,
      accumulated,
      policy: b.policy,
      priorUnits: units,
      units: input.units,
      basisOrdinal: basis.ordinal,
      basisAccumulated: decimalToUnits(baseline?.accumulated ?? "0.00"),
      basisUnits: baseline?.units ?? 0,
    });
    return {
      chartId: b.chartId,
      amount: unitsToDecimal(amount),
      accumulated: unitsToDecimal(accumulated + amount),
      units: units + input.units,
    };
  });
  const primary = books.find((b) => b.chartId === a.chart_id)!,
    previousPrimary =
      history.at(-1) && Number(history.at(-1)!.ordinal) > basis.ordinal
        ? (
            JSON.parse(String(history.at(-1)!.result_json)) as Result
          ).books.find((b) => b.chartId === a.chart_id)!.accumulated
        : (basis.books.find((b) => b.chartId === a.chart_id)?.accumulated ??
          "0.00");
  if (
    BigInt(a.accumulated_depreciation_minor!) !==
    decimalToUnits(previousPrimary)
  )
    throw new ApiError(
      409,
      "ASSET_HISTORY_RECONCILIATION_REQUIRED",
      "The asset carrying value differs from its retained policy history.",
    );
  const result: Result = { ordinal, units: input.units, books };
  return {
    a,
    definition,
    book,
    result,
    primary,
    hash: await sha256(
      JSON.stringify({ p, input, a, history, book, result, carrying }),
    ),
  };
}
async function journals(
  db: SqlDatabase,
  actor: WorkContext,
  runId: string,
  input: z.infer<typeof assetPolicyRunSchema>,
  e: Awaited<ReturnType<typeof runEvidence>>,
  key: string,
  status: "draft" | "posted",
) {
  const output = [] as Array<{
    id: string;
    sourceType: "asset_policy_depreciation" | "book_adjustment";
    steps: SqlStatement[];
  }>;
  for (const b of e.book.books) {
    const amount = decimalToUnits(
        e.result.books.find((r) => r.chartId === b.chartId)!.amount,
      ),
      delta = b.ruleId ? amount - decimalToUnits(e.primary.amount) : amount;
    if (delta === BigInt(0)) continue;
    const positive = delta > BigInt(0),
      absolute = positive ? delta : -delta,
      sourceType = b.ruleId ? "book_adjustment" : "asset_policy_depreciation",
      lines: JournalLineInput[] = [
        {
          accountId: b.expenseAccount,
          debit: unitsToDecimal(positive ? absolute : BigInt(0)),
          credit: unitsToDecimal(positive ? BigInt(0) : absolute),
          dimensions: e.definition.dimensions,
        },
        {
          accountId: b.accumulatedAccount,
          debit: unitsToDecimal(positive ? BigInt(0) : absolute),
          credit: unitsToDecimal(positive ? absolute : BigInt(0)),
          dimensions: e.definition.dimensions,
        },
      ],
      j = await prepareJournal(
        db,
        actor,
        {
          chartId: b.chartId,
          periodId: input.periodId,
          entryDate: input.entryDate,
          currency: String(e.a.currency),
          memo: input.reason,
          sourceType,
          sourceId: runId,
          isOpening: false,
          lines,
        },
        `${key}:${output.length}`,
        status,
      );
    output.push({ id: j.result.id, sourceType, steps: j.steps });
  }
  return output;
}
export async function prepareAssetPolicyRun(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof assetPolicyRunSchema>,
  key: string,
  ownedTransaction?: SqlDatabase,
) {
  actor = requirePermission(actor, "finance.write");
  const input = assetPolicyRunSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    runId = `asset-policy-run-${key}`,
    op = "finance.asset-policy-run.prepare";
  return financeOwnedTransaction(write, ownedTransaction, async (db) => {
    const prior = await replayCommand(db, actor, key, op, runId, { id, input });
    if (prior) return prior;
    await requireFinanceTool(db, actor.companyId, "asset_policies");
    const p = await edition(db, actor, id),
      e = await runEvidence(db, actor, p, input);
    await journals(db, actor, runId, input, e, key, "draft");
    const now = Date.now();
    return executeCommand(
      db,
      actor,
      key,
      op,
      { id, input },
      runId,
      1,
      { id: runId, status: "draft", version: 1, result: e.result },
      [
        db
          .prepare(
            "INSERT INTO finance_asset_policy_runs(id,company_id,asset_id,policy_id,ordinal,entry_date,input_json,result_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            runId,
            actor.companyId,
            e.a.id,
            id,
            e.result.ordinal,
            input.entryDate,
            JSON.stringify(input),
            JSON.stringify(e.result),
            e.hash,
            actor.userId,
            input.reason,
            now,
            now,
          ),
      ],
    );
  });
}
async function getRun(db: SqlDatabase, actor: WorkContext, id: string) {
  const r = await db
    .prepare(
      "SELECT * FROM finance_asset_policy_runs WHERE id=? AND company_id=?",
    )
    .bind(id, actor.companyId)
    .first<Row>();
  if (!r)
    throw new ApiError(
      404,
      "ASSET_POLICY_RUN_NOT_FOUND",
      "Depreciation draft not found.",
    );
  return r;
}
export async function decideAssetPolicyRun(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof assetPolicyDecisionSchema>,
  action: "post" | "reject",
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  const input = assetPolicyDecisionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = `finance.asset-policy-run.${action}`;
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const r = await getRun(db, actor, id);
    if (r.status !== "draft" || r.version !== input.expectedVersion)
      throw new ApiError(
        409,
        "ASSET_POLICY_RUN_CHANGED",
        "Refresh the depreciation draft before deciding.",
      );
    if (samePerson(actor, r.prepared_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must review depreciation.",
      );
    const p = await edition(db, actor, String(r.policy_id)),
      definition = assetPolicyRunSchema.parse(JSON.parse(String(r.input_json))),
      e =
        action === "post" ? await runEvidence(db, actor, p, definition) : null;
    if (e && e.hash !== r.source_hash)
      throw new ApiError(
        409,
        "ASSET_POLICY_RUN_SOURCE_CHANGED",
        "Asset, policy or book evidence changed. Reject and prepare again.",
      );
    const js = e
        ? await journals(db, actor, id, definition, e, key, "posted")
        : [],
      now = Date.now(),
      steps: SqlStatement[] = js.flatMap((j) => j.steps);
    if (e)
      steps.push(
        db
          .prepare(
            "UPDATE finance_assets SET accumulated_depreciation_minor=?,version=version+1,updated_at=? WHERE id=?",
          )
          .bind(String(decimalToUnits(e.primary.accumulated)), now, e.a.id),
        db
          .prepare(
            "UPDATE finance_asset_policy_editions SET version=version+1,updated_at=? WHERE id=?",
          )
          .bind(now, p.id),
        db
          .prepare(
            "INSERT INTO finance_asset_events(id,asset_id,company_id,event_type,asset_version,command_id,payload_json,actor_id,created_at) VALUES(?,?,?,'depreciated',?,?,?,?,?)",
          )
          .bind(
            `asset-event-${key}`,
            e.a.id,
            actor.companyId,
            Number(e.a.version) + 1,
            key,
            JSON.stringify({
              policyId: p.id,
              runId: id,
              ...e.result,
              journalIds: js.map((j) => j.id),
            }),
            actor.userId,
            now,
          ),
      );
    steps.push(
      assertion(
        db,
        key,
        "asset-policy-run-version",
        "EXISTS(SELECT 1 FROM finance_asset_policy_runs WHERE id=? AND company_id=? AND status='draft' AND version=?)",
        [id, actor.companyId, input.expectedVersion],
      ),
      db
        .prepare(
          "UPDATE finance_asset_policy_runs SET status=?,reviewed_by=?,journal_ids_json=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          action === "post" ? "posted" : "rejected",
          actor.userId,
          JSON.stringify(
            js.map((j) => ({ id: j.id, sourceType: j.sourceType })),
          ),
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
      input.expectedVersion + 1,
      {
        id,
        status: action === "post" ? "posted" : "rejected",
        version: input.expectedVersion + 1,
        journalIds: js.map((j) => j.id),
      },
      steps,
    );
  });
}
export async function reverseAssetPolicyRun(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof assetPolicyReversalSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  const input = assetPolicyReversalSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = "finance.asset-policy-run.reverse";
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const r = await getRun(db, actor, id),
      p = await edition(db, actor, String(r.policy_id)),
      a = await asset(db, actor, String(r.asset_id));
    if (
      r.status !== "posted" ||
      r.version !== input.expectedVersion ||
      p.status !== "active"
    )
      throw new ApiError(
        409,
        "ASSET_POLICY_RUN_CHANGED",
        "Reverse the latest depreciation in the current policy edition.",
      );
    if (samePerson(actor, r.prepared_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "An independent Finance approver must reverse depreciation.",
      );
    if (
      (await db
        .prepare(
          "SELECT 1 FROM finance_asset_policy_runs WHERE asset_id=? AND ((status='posted' AND ordinal>?) OR status='draft')",
        )
        .bind(a.id, r.ordinal)
        .first()) ||
      (await db
        .prepare(
          "SELECT 1 FROM finance_asset_policy_editions WHERE asset_id=? AND status='draft'",
        )
        .bind(a.id)
        .first())
    )
      throw new ApiError(
        409,
        "ASSET_POLICY_LATER_WORK",
        "Decide policy drafts and reverse later depreciation first.",
      );
    const owned = JSON.parse(String(r.journal_ids_json)) as Array<{
        id: string;
        sourceType: "asset_policy_depreciation" | "book_adjustment";
      }>,
      reversals = [];
    for (const [i, j] of owned.entries())
      reversals.push(
        await prepareOwnedJournalReversal(
          db,
          actor,
          j.id,
          j.sourceType,
          id,
          input,
          `${key}:${i}`,
        ),
      );
    const result = JSON.parse(String(r.result_json)) as Result,
      primary = result.books.find((b) => b.chartId === a.chart_id)!;
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      input.expectedVersion + 1,
      {
        id,
        status: "reversed",
        version: input.expectedVersion + 1,
        reversalJournalIds: reversals.map((j) => j.id),
      },
      [
        ...reversals.flatMap((j) => j.steps),
        db
          .prepare(
            "UPDATE finance_assets SET accumulated_depreciation_minor=accumulated_depreciation_minor-?,version=version+1,updated_at=? WHERE id=?",
          )
          .bind(String(decimalToUnits(primary.amount)), Date.now(), a.id),
        db
          .prepare(
            "UPDATE finance_asset_policy_editions SET version=version+1,updated_at=? WHERE id=?",
          )
          .bind(Date.now(), p.id),
        db
          .prepare(
            "UPDATE finance_asset_policy_runs SET status='reversed',reversal_ids_json=?,reversal_entry_date=?,version=version+1,updated_at=? WHERE id=?",
          )
          .bind(
            JSON.stringify(reversals.map((j) => j.id)),
            input.entryDate,
            Date.now(),
            id,
          ),
      ],
    );
  });
}
export async function assetPolicyWorkspace(actor: WorkContext) {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId),
    [policies, runs, assets, books, periods] = await Promise.all([
      write
        .prepare(
          "SELECT * FROM finance_asset_policy_editions WHERE company_id=? ORDER BY created_at DESC LIMIT 500",
        )
        .bind(actor.companyId)
        .all<Row>(),
      write
        .prepare(
          "SELECT * FROM finance_asset_policy_runs WHERE company_id=? ORDER BY created_at DESC LIMIT 500",
        )
        .bind(actor.companyId)
        .all<Row>(),
      write
        .prepare(
          "SELECT a.*,c.chart_id,(SELECT count(*) FROM finance_asset_depreciations old WHERE old.asset_id=a.id)+(SELECT count(*) FROM finance_asset_policy_runs run WHERE run.asset_id=a.id AND run.status='posted') completed_depreciation_months FROM finance_assets a JOIN finance_accounts c ON c.id=a.account_id WHERE a.company_id=? AND a.status='in_service' ORDER BY asset_number",
        )
        .bind(actor.companyId)
        .all<Row>(),
      write
        .prepare(
          "SELECT c.id,c.name,r.primary_chart_id FROM finance_charts c LEFT JOIN (SELECT DISTINCT secondary_chart_id,primary_chart_id FROM finance_book_rules WHERE status='approved') r ON r.secondary_chart_id=c.id WHERE c.company_id=? AND c.status='active' ORDER BY c.id",
        )
        .bind(actor.companyId)
        .all<Row>(),
      write
        .prepare(
          "SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date",
        )
        .bind(actor.companyId)
        .all<Row>(),
    ]);
  return {
    policies: policies.results,
    runs: runs.results,
    assets: assets.results,
    books: books.results,
    periods: periods.results,
  };
}

/** Disposal removes each book's carrying value; it never leaves secondary depreciation behind. */
export async function prepareAssetPolicyDisposal(
  db: SqlDatabase,
  actor: WorkContext,
  assetId: string,
  input: {
    id: string;
    periodId: string;
    entryDate: string;
    proceeds: number;
    gainAccountId: string;
    lossAccountId: string;
  },
  key: string,
  status: "draft" | "posted",
) {
  const p = await db
    .prepare(
      "SELECT * FROM finance_asset_policy_editions WHERE asset_id=? AND company_id=? AND status='active'",
    )
    .bind(assetId, actor.companyId)
    .first<Row>();
  if (!p)
    return {
      hash: null,
      dimensions: {},
      steps: [] as SqlStatement[],
      journalIds: [] as string[],
    };
  const a = await db
    .prepare(
      "SELECT a.*,c.chart_id FROM finance_assets a JOIN finance_accounts c ON c.id=a.account_id WHERE a.id=? AND a.company_id=? AND a.status='in_service'",
    )
    .bind(assetId, actor.companyId)
    .first<Row>();
  if (!a)
    throw new ApiError(
      409,
      "ASSET_NOT_RECOGNIZED",
      "The asset carrying value is unavailable for disposal.",
    );
  if (
    (await db
      .prepare(
        "SELECT 1 FROM finance_asset_policy_runs WHERE asset_id=? AND status='draft'",
      )
      .bind(assetId)
      .first()) ||
    (await db
      .prepare(
        "SELECT 1 FROM finance_asset_policy_editions WHERE asset_id=? AND status='draft'",
      )
      .bind(assetId)
      .first())
  )
    throw new ApiError(
      409,
      "ASSET_POLICY_PENDING",
      "Decide depreciation and policy drafts before disposal.",
    );
  const definition = assetPolicySchema.parse(
      JSON.parse(String(p.content_json)),
    ),
    history = await postedHistory(db, a),
    book = await bookEvidence(db, actor, a, definition, input.entryDate),
    last = history.at(-1),
    carrying = last
      ? (JSON.parse(String(last.result_json)) as Result).books
      : p.basis_json
        ? (JSON.parse(String(p.basis_json)) as AssetCarryBasis).books
        : definition.books.map((b) => ({
            chartId: b.chartId,
            accumulated: "0.00",
          }));
  if (last && String(last.entry_date) > input.entryDate)
    throw new ApiError(
      422,
      "ASSET_DISPOSAL_DATE",
      "Disposal cannot precede retained book depreciation.",
    );
  const primaryAccum = BigInt(a.accumulated_depreciation_minor!),
    primaryGain = BigInt(input.proceeds) - BigInt(a.cost_minor!) + primaryAccum,
    steps: SqlStatement[] = [],
    journalIds: string[] = [],
    mappingEvidence = [];
  for (const b of book.books.filter((b) => b.ruleId)) {
    const retained = carrying.find((c) => c.chartId === b.chartId);
    if (!retained)
      throw new ApiError(
        409,
        "ASSET_BOOK_CARRYING_REQUIRED",
        "Review the asset's secondary-book carrying value before disposal.",
      );
    const delta = decimalToUnits(retained.accumulated) - primaryAccum,
      secondaryGain = primaryGain + delta;
    const maps = (
        await db
          .prepare(
            "SELECT m.primary_account_id,m.secondary_account_id,a.version,a.status FROM finance_book_account_mappings m JOIN finance_accounts a ON a.id=m.secondary_account_id WHERE m.rule_id=? AND m.primary_account_id IN (?,?) ORDER BY m.primary_account_id",
          )
          .bind(b.ruleId, input.gainAccountId, input.lossAccountId)
          .all<Row>()
      ).results,
      byId = new Map(
        maps.map((m) => [m.primary_account_id, String(m.secondary_account_id)]),
      );
    if (maps.length !== 2 || maps.some((m) => m.status !== "active"))
      throw new ApiError(
        409,
        "ASSET_DISPOSAL_BOOK_MAPPING_REQUIRED",
        "Map the disposal gain and loss accounts in each book.",
      );
    mappingEvidence.push(maps);
    const balances = new Map<string, bigint>();
    function add(id: string, value: bigint) {
      balances.set(id, (balances.get(id) ?? BigInt(0)) + value);
    }
    add(b.accumulatedAccount, delta);
    if (primaryGain > BigInt(0))
      add(byId.get(input.gainAccountId)!, primaryGain);
    else if (primaryGain < BigInt(0))
      add(byId.get(input.lossAccountId)!, primaryGain);
    if (secondaryGain > BigInt(0))
      add(byId.get(input.gainAccountId)!, -secondaryGain);
    else if (secondaryGain < BigInt(0))
      add(byId.get(input.lossAccountId)!, -secondaryGain);
    const lines = [...balances]
      .filter(([, amount]) => amount !== BigInt(0))
      .map(([accountId, amount]) => ({
        accountId,
        debit: unitsToDecimal(amount > BigInt(0) ? amount : BigInt(0)),
        credit: unitsToDecimal(amount < BigInt(0) ? -amount : BigInt(0)),
        dimensions: definition.dimensions,
      }));
    if (!lines.length) continue;
    const journal = await prepareJournal(
      db,
      actor,
      {
        chartId: b.chartId,
        periodId: input.periodId,
        entryDate: input.entryDate,
        currency: String(a.currency),
        memo: "Retire secondary-book asset carrying value",
        sourceType: "book_adjustment",
        sourceId: input.id,
        isOpening: false,
        lines,
      },
      `${key}:disposal:${journalIds.length}`,
      status,
    );
    steps.push(...journal.steps);
    journalIds.push(journal.result.id);
  }
  return {
    hash: await sha256(
      JSON.stringify({ p, a, book, history, mappingEvidence, input }),
    ),
    dimensions: definition.dimensions,
    steps,
    journalIds,
  };
}
