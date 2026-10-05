import "server-only";
import { z } from "zod";
import {
  instrumentSchema,
  instrumentSchedule,
  halfAway,
  type InstrumentInput,
} from "@/lib/finance-instrument-policy";
import { decimalToUnits, unitsToDecimal } from "@/lib/finance-money";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "./authorization";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { executeCommand, replayCommand, assertion } from "./commands";
import { financeTransaction, financeOwnedTransaction } from "./finance-transactions";
import { requireFinanceTool } from "./finance-company-tools";
import { requireFinanceMoneyScale } from "./finance-money-policy";
import { prepareJournal, type JournalLineInput } from "./finance";
import { prepareOwnedJournalReversal } from "./finance-owned-reversal";
import { sha256 } from "./crypto";
import { ApiError } from "./http";

type Row = Record<string, string | number | null>;
type Schedule = ReturnType<typeof instrumentSchedule> & { rightOfUse?: string };
export const instrumentEditionSchema = instrumentSchema.safeExtend({
  periodId: z.string().min(1),
  parentId: z.string().min(1).optional(),
  parentVersion: z.number().int().positive().optional(),
  bankTransactionId: z.string().min(1).optional(),
  bankVersion: z.number().int().positive().optional(),
});
export const instrumentDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});
export const instrumentRunSchema = z.object({
  expectedVersion: z.number().int().positive(),
  kind: z.enum(["payment", "payoff"]).default("payment"),
  periodId: z.string().min(1),
  entryDate: z.string().date(),
  bankTransactionId: z.string().min(1),
  bankVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});
export const instrumentReversalSchema = instrumentDecisionSchema.extend({
  periodId: z.string().min(1),
  entryDate: z.string().date(),
});
async function get(db: SqlDatabase, actor: WorkContext, id: string) {
  const row = await db
    .prepare(
      "SELECT * FROM finance_instrument_editions WHERE id=? AND company_id=?",
    )
    .bind(id, actor.companyId)
    .first<Row>();
  if (!row)
    throw new ApiError(
      404,
      "INSTRUMENT_NOT_FOUND",
      "Financing agreement not found.",
    );
  return row;
}
function terms(row: Row) {
  return instrumentEditionSchema.parse(JSON.parse(String(row.content_json)));
}
function schedule(row: Row) {
  return JSON.parse(String(row.schedule_json)) as Schedule;
}
async function configuration(
  db: SqlDatabase,
  actor: WorkContext,
  input: InstrumentInput,
) {
  await requireFinanceMoneyScale(db, actor.companyId, input.currency);
  const chart = await db
    .prepare(
      "SELECT id,base_currency,status FROM finance_charts WHERE id=? AND company_id=? AND status='active'",
    )
    .bind(input.chartId, actor.companyId)
    .first<Row>();
  if (
    !chart ||
    chart.base_currency !== input.currency ||
    (await db
      .prepare(
        "SELECT 1 FROM finance_book_rules WHERE company_id=? AND secondary_chart_id=? AND status='approved'",
      )
      .bind(actor.companyId, input.chartId)
      .first())
  )
    throw new ApiError(
      422,
      "INSTRUMENT_PRIMARY_BOOK_REQUIRED",
      "Use the active primary book and its functional currency.",
    );
  const ids = [
    input.bankAccountId,
    input.liabilityAccountId,
    input.interestAccountId,
    ...(input.kind === "lease"
      ? [
          input.rightOfUseAccountId!,
          input.accumulatedAccountId!,
          input.depreciationAccountId!,
        ]
      : []),
  ];
  if (new Set(ids).size !== ids.length)
    throw new ApiError(
      422,
      "INSTRUMENT_ACCOUNTS_REQUIRED",
      "Financing mappings must use distinct accounts.",
    );
  const accounts = (
      await db
        .prepare(
          `SELECT id,version,currency,category,control_kind FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND id IN (${ids.map(() => "?").join(",")}) ORDER BY id`,
        )
        .bind(actor.companyId, input.chartId, ...ids)
        .all<Row>()
    ).results,
    byId = new Map(accounts.map((a) => [a.id, a]));
  if (
    accounts.length !== ids.length ||
    accounts.some((a) => a.currency !== input.currency) ||
    byId.get(input.bankAccountId)?.category !== "asset" ||
    byId.get(input.bankAccountId)?.control_kind !== "bank" ||
    byId.get(input.liabilityAccountId)?.category !== "liability" ||
    byId.get(input.liabilityAccountId)?.control_kind !== "none" ||
    byId.get(input.interestAccountId)?.category !== "expense" ||
    byId.get(input.interestAccountId)?.control_kind !== "none" ||
    (input.kind === "lease" &&
      [
        input.rightOfUseAccountId!,
        input.accumulatedAccountId!,
        input.depreciationAccountId!,
      ].some(
        (id, i) =>
          byId.get(id)?.category !== (i === 2 ? "expense" : "asset") ||
          byId.get(id)?.control_kind !== "none",
      ))
  )
    throw new ApiError(
      422,
      "INSTRUMENT_ACCOUNTS_REQUIRED",
      "Map same-book bank, liability and expense accounts, with separate noncontrol right-of-use accounts for a lease.",
    );
  return { chart, accounts };
}
async function bankEvidence(
  db: SqlDatabase,
  actor: WorkContext,
  input: InstrumentInput,
  bankId: string | undefined,
  version: number | undefined,
  signed: bigint,
  date: string,
) {
  if (!bankId || !version)
    throw new ApiError(
      422,
      "INSTRUMENT_BANK_EVIDENCE_REQUIRED",
      "Select an exact imported bank movement and its version.",
    );
  const bank = await db
    .prepare(
      "SELECT t.*,s.bank_account,s.source_hash FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=?",
    )
    .bind(bankId, actor.companyId)
    .first<Row>();
  if (
    !bank ||
    bank.version !== version ||
    bank.match_status !== "unmatched" ||
    bank.bank_account !== input.bankAccountId ||
    bank.currency !== input.currency ||
    BigInt(bank.amount_minor!) !== signed ||
    bank.transaction_date !== date
  )
    throw new ApiError(
      409,
      "INSTRUMENT_BANK_EVIDENCE_CHANGED",
      "Select an unmatched movement with the exact account, currency, date and cash amount.",
    );
  return bank;
}
async function history(db: SqlDatabase, row: Row) {
  const rows = (
    await db
      .prepare(
        "SELECT r.id,r.ordinal,r.result_json,r.source_hash,j.status journal_status,j.entry_date FROM finance_instrument_runs r JOIN finance_journals j ON j.id=r.journal_id WHERE r.instrument_id=? AND r.status='posted' ORDER BY r.ordinal",
      )
      .bind(row.id)
      .all<Row>()
  ).results;
  if (rows.some((r, i) => r.ordinal !== i + 1 || r.journal_status !== "posted"))
    throw new ApiError(
      409,
      "INSTRUMENT_HISTORY_CHANGED",
      "Reconcile the retained financing history before preparing more work.",
    );
  return rows;
}
function carrying(row: Row, rows: Row[]) {
  const s = schedule(row),
    last = rows.at(-1),
    result = last
      ? (JSON.parse(String(last.result_json)) as Schedule["rows"][number])
      : null;
  return {
    liability: decimalToUnits(result?.closing ?? s.principal),
    rightOfUse:
      decimalToUnits(s.rightOfUse ?? s.principal) -
      rows.reduce(
        (sum, r) =>
          sum + decimalToUnits(JSON.parse(String(r.result_json)).depreciation),
        BigInt(0),
      ),
    lastDate: last ? String(last.entry_date) : terms(row).startDate,
  };
}
async function editionEvidence(
  db: SqlDatabase,
  actor: WorkContext,
  input: z.infer<typeof instrumentEditionSchema>,
) {
  const config = await configuration(db, actor, input),
    calculation = instrumentSchedule(input),
    parent = input.parentId ? await get(db, actor, input.parentId) : null;
  let priorRows: Row[] = [],
    adjustment = decimalToUnits(calculation.principal);
  if (parent) {
    if (
      input.kind !== "lease" ||
      parent.kind !== "lease" ||
      parent.status !== "active" ||
      parent.version !== input.parentVersion
    )
      throw new ApiError(
        409,
        "LEASE_AMENDMENT_CHANGED",
        "Amend the current active lease edition using its current version.",
      );
    const old = terms(parent);
    if (
      [
        "number",
        "chartId",
        "currency",
        "bankAccountId",
        "liabilityAccountId",
        "rightOfUseAccountId",
        "accumulatedAccountId",
        "depreciationAccountId",
        "interestAccountId",
      ].some(
        (k) => old[k as keyof typeof old] !== input[k as keyof typeof input],
      )
    )
      throw new ApiError(
        422,
        "LEASE_AMENDMENT_MAPPING",
        "Retain the original book and accounts when remeasuring a lease.",
      );
    priorRows = await history(db, parent);
    const c = carrying(parent, priorRows);
    if (input.startDate !== c.lastDate || c.liability <= BigInt(0))
      throw new ApiError(
        422,
        "LEASE_AMENDMENT_BOUNDARY",
        "Remeasure at the last settled boundary, before preparing its next payment.",
      );
    if (
      await db
        .prepare(
          "SELECT 1 FROM finance_instrument_runs WHERE instrument_id=? AND status='draft'",
        )
        .bind(parent.id)
        .first()
    )
      throw new ApiError(
        409,
        "LEASE_PAYMENT_PENDING",
        "Decide existing payment drafts before remeasurement.",
      );
    adjustment = decimalToUnits(calculation.principal) - c.liability;
    const rightOfUse = c.rightOfUse + adjustment;
    if (rightOfUse < BigInt(0))
      throw new ApiError(
        422,
        "LEASE_REMEASUREMENT_NEGATIVE_ASSET",
        "The revised liability cannot reduce the right-of-use carrying amount below zero under this policy.",
      );
    // Remeasurement changes liability by PV difference; ROU retains its existing carrying amount plus that difference.
    let previous = BigInt(0);
    for (const r of calculation.rows) {
      const target = (rightOfUse * BigInt(r.ordinal)) / BigInt(input.months);
      r.depreciation = unitsToDecimal(target - previous);
      previous = target;
    }
    Object.assign(calculation, { rightOfUse: unitsToDecimal(rightOfUse) });
  }
  const bank =
    input.kind === "loan"
      ? await bankEvidence(
          db,
          actor,
          input,
          input.bankTransactionId,
          input.bankVersion,
          decimalToUnits(calculation.principal),
          input.startDate,
        )
      : null;
  const hash = await sha256(
    JSON.stringify({ input, config, parent, priorRows, bank, calculation }),
  );
  return { calculation, parent, adjustment, bank, hash };
}
function line(
  accountId: string,
  amount: bigint,
  debit: boolean,
  dimensions: Record<string, string>,
): JournalLineInput {
  return {
    accountId,
    debit: unitsToDecimal(debit ? amount : BigInt(0)),
    credit: unitsToDecimal(debit ? BigInt(0) : amount),
    dimensions,
  };
}
async function activationJournal(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  input: z.infer<typeof instrumentEditionSchema>,
  e: Awaited<ReturnType<typeof editionEvidence>>,
  key: string,
  status: "draft" | "posted",
) {
  if (e.adjustment === BigInt(0)) return null;
  const positive = e.adjustment > BigInt(0),
    amount = positive ? e.adjustment : -e.adjustment;
  return prepareJournal(
    db,
    actor,
    {
      chartId: input.chartId,
      periodId: input.periodId,
      entryDate: input.startDate,
      currency: input.currency,
      memo: input.reason,
      sourceType: "instrument_activation",
      sourceId: id,
      isOpening: false,
      lines: [
        line(
          input.kind === "lease"
            ? input.rightOfUseAccountId!
            : input.bankAccountId,
          amount,
          positive,
          input.dimensions,
        ),
        line(input.liabilityAccountId, amount, !positive, input.dimensions),
      ],
    },
    key,
    status,
  );
}
export async function prepareInstrument(
  actor: WorkContext,
  raw: z.input<typeof instrumentEditionSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  const input = instrumentEditionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    id = `instrument-${key}`,
    op = "finance.instrument.prepare";
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    await requireFinanceTool(db, actor.companyId, "instruments");
    const e = await editionEvidence(db, actor, input);
    await activationJournal(db, actor, id, input, e, key, "draft");
    const now = Date.now();
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      1,
      { id, status: "draft", version: 1, principal: e.calculation.principal },
      [
        db
          .prepare(
            "INSERT INTO finance_instrument_editions(id,company_id,root_id,parent_id,kind,number,name,chart_id,edition,content_json,schedule_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            actor.companyId,
            e.parent?.root_id ?? id,
            e.parent?.id ?? null,
            input.kind,
            input.number,
            input.name,
            input.chartId,
            Number(e.parent?.edition ?? 0) + 1,
            JSON.stringify(input),
            JSON.stringify(e.calculation),
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
export async function decideInstrument(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof instrumentDecisionSchema>,
  action: "approve" | "reject",
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  const input = instrumentDecisionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = `finance.instrument.${action}`;
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const row = await get(db, actor, id);
    if (row.status !== "draft" || row.version !== input.expectedVersion)
      throw new ApiError(
        409,
        "INSTRUMENT_CHANGED",
        "Refresh the financing draft before deciding.",
      );
    if (samePerson(actor, row.prepared_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must review the financing terms.",
      );
    const content = terms(row),
      e =
        action === "approve" ? await editionEvidence(db, actor, content) : null;
    if (e && e.hash !== row.source_hash)
      throw new ApiError(
        409,
        "INSTRUMENT_SOURCE_CHANGED",
        "Financing terms, bank evidence or mappings changed. Reject and prepare again.",
      );
    const journal = e
        ? await activationJournal(db, actor, id, content, e, key, "posted")
        : null,
      steps: SqlStatement[] = [];
    if (e?.parent)
      steps.push(
        db
          .prepare(
            "UPDATE finance_instrument_editions SET status='superseded',version=version+1,updated_at=? WHERE id=?",
          )
          .bind(Date.now(), e.parent.id),
      );
    if (journal) steps.push(...journal.steps);
    if (e?.bank && journal)
      steps.push(
        db
          .prepare(
            "UPDATE finance_bank_transactions SET match_status='matched',matched_type='journal',matched_id=?,version=version+1 WHERE id=?",
          )
          .bind(journal.result.id, e.bank.id),
      );
    steps.push(
      assertion(
        db,
        key,
        "instrument-version",
        "EXISTS(SELECT 1 FROM finance_instrument_editions WHERE id=? AND company_id=? AND version=? AND status='draft')",
        [id, actor.companyId, input.expectedVersion],
      ),
      db
        .prepare(
          "UPDATE finance_instrument_editions SET status=?,reviewed_by=?,activation_journal_id=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          action === "approve" ? "active" : "rejected",
          actor.userId,
          journal?.result.id ?? null,
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
        journalId: journal?.result.id ?? null,
      },
      steps,
    );
  });
}
async function runEvidence(
  db: SqlDatabase,
  actor: WorkContext,
  row: Row,
  input: z.infer<typeof instrumentRunSchema>,
) {
  if (row.status !== "active" || row.version !== input.expectedVersion)
    throw new ApiError(
      409,
      "INSTRUMENT_CHANGED",
      "Use the current active agreement version.",
    );
  const content = terms(row),
    config = await configuration(db, actor, content),
    rows = await history(db, row),
    s = schedule(row),
    ordinal = rows.length + 1;
  if (ordinal > s.rows.length)
    throw new ApiError(
      409,
      "INSTRUMENT_COMPLETE",
      "The financing schedule is complete.",
    );
  let result = { ...s.rows[ordinal - 1] };
  if (input.kind === "payoff") {
    if (content.kind !== "loan")
      throw new ApiError(
        422,
        "LOAN_PAYOFF_REQUIRED",
        "This payoff command is available for loans.",
      );
    const c = carrying(row, rows);
    if (input.entryDate < c.lastDate || input.entryDate > result.dueDate)
      throw new ApiError(
        422,
        "LOAN_PAYOFF_DATE",
        "Settle past due instalments first; payoff must fall between the last settled date and next due date.",
      );
    const days = BigInt(
        Math.round(
          (Date.parse(input.entryDate) - Date.parse(c.lastDate)) / 86400000,
        ),
      ),
      interest = halfAway(
        c.liability * BigInt(content.annualRateBps) * days,
        BigInt(3650000),
      );
    result = {
      ...result,
      dueDate: input.entryDate,
      opening: unitsToDecimal(c.liability),
      payment: unitsToDecimal(c.liability + interest),
      interest: unitsToDecimal(interest),
      principal: unitsToDecimal(c.liability),
      closing: "0.00",
      depreciation: "0.00",
    };
  } else if (input.entryDate < result.dueDate)
    throw new ApiError(
      422,
      "INSTRUMENT_NOT_DUE",
      "Post a scheduled instalment on or after its due date.",
    );
  if (rows.at(-1) && input.entryDate < String(rows.at(-1)!.entry_date))
    throw new ApiError(
      422,
      "INSTRUMENT_SEQUENCE",
      "Payments must follow their retained accounting dates.",
    );
  const bank = await bankEvidence(
      db,
      actor,
      content,
      input.bankTransactionId,
      input.bankVersion,
      -decimalToUnits(result.payment),
      input.entryDate,
    ),
    hash = await sha256(
      JSON.stringify({ row, config, rows, input, result, bank }),
    );
  const lines = [
    line(
      content.liabilityAccountId,
      decimalToUnits(result.principal),
      true,
      content.dimensions,
    ),
    line(
      content.bankAccountId,
      decimalToUnits(result.payment),
      false,
      content.dimensions,
    ),
  ];
  if (decimalToUnits(result.interest) > BigInt(0))
    lines.push(
      line(
        content.interestAccountId,
        decimalToUnits(result.interest),
        true,
        content.dimensions,
      ),
    );
  if (decimalToUnits(result.depreciation) > BigInt(0))
    lines.push(
      line(
        content.depreciationAccountId!,
        decimalToUnits(result.depreciation),
        true,
        content.dimensions,
      ),
      line(
        content.accumulatedAccountId!,
        decimalToUnits(result.depreciation),
        false,
        content.dimensions,
      ),
    );
  return { content, result, ordinal, hash, lines };
}
export async function prepareInstrumentRun(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof instrumentRunSchema>,
  key: string,
  ownedTransaction?:SqlDatabase,
) {
  actor = requirePermission(actor, "finance.write");
  const input = instrumentRunSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    runId = `instrument-run-${key}`,
    op = "finance.instrument-run.prepare";
  return financeOwnedTransaction(write, ownedTransaction, async (db) => {
    const prior = await replayCommand(db, actor, key, op, runId, { id, input });
    if (prior) return prior;
    await requireFinanceTool(db, actor.companyId, "instruments");
    const row = await get(db, actor, id),
      e = await runEvidence(db, actor, row, input);
    await prepareJournal(
      db,
      actor,
      {
        chartId: e.content.chartId,
        periodId: input.periodId,
        entryDate: input.entryDate,
        currency: e.content.currency,
        memo: input.reason,
        sourceType: "instrument_payment",
        sourceId: runId,
        isOpening: false,
        lines: e.lines,
      },
      key,
    );
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
            "INSERT INTO finance_instrument_runs(id,company_id,instrument_id,kind,ordinal,input_json,result_json,source_hash,bank_transaction_id,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            runId,
            actor.companyId,
            id,
            input.kind,
            e.ordinal,
            JSON.stringify(input),
            JSON.stringify(e.result),
            e.hash,
            input.bankTransactionId,
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
  const row = await db
    .prepare(
      "SELECT * FROM finance_instrument_runs WHERE id=? AND company_id=?",
    )
    .bind(id, actor.companyId)
    .first<Row>();
  if (!row)
    throw new ApiError(
      404,
      "INSTRUMENT_RUN_NOT_FOUND",
      "Financing payment draft not found.",
    );
  return row;
}
export async function decideInstrumentRun(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof instrumentDecisionSchema>,
  action: "post" | "reject",
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  const input = instrumentDecisionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = `finance.instrument-run.${action}`;
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const run = await getRun(db, actor, id);
    if (run.status !== "draft" || run.version !== input.expectedVersion)
      throw new ApiError(
        409,
        "INSTRUMENT_RUN_CHANGED",
        "Refresh the payment draft before deciding.",
      );
    if (samePerson(actor, run.prepared_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must review the bank evidence and repayment.",
      );
    const row = await get(db, actor, String(run.instrument_id)),
      definition = instrumentRunSchema.parse(
        JSON.parse(String(run.input_json)),
      ),
      e =
        action === "post"
          ? await runEvidence(db, actor, row, definition)
          : null;
    if (e && e.hash !== run.source_hash)
      throw new ApiError(
        409,
        "INSTRUMENT_RUN_SOURCE_CHANGED",
        "Schedule, mappings or bank evidence changed. Reject and prepare a fresh draft.",
      );
    const journal = e
      ? await prepareJournal(
          db,
          actor,
          {
            chartId: e.content.chartId,
            periodId: definition.periodId,
            entryDate: definition.entryDate,
            currency: e.content.currency,
            memo: input.reason,
            sourceType: "instrument_payment",
            sourceId: id,
            isOpening: false,
            lines: e.lines,
          },
          key,
          "posted",
        )
      : null;
    const steps = journal
      ? [
          ...journal.steps,
          db
            .prepare(
              "UPDATE finance_bank_transactions SET match_status='matched',matched_type='journal',matched_id=?,version=version+1 WHERE id=?",
            )
            .bind(journal.result.id, run.bank_transaction_id),
          db
            .prepare(
              "UPDATE finance_instrument_editions SET status=?,version=version+1,updated_at=? WHERE id=?",
            )
            .bind(
              e!.result.closing === "0.00" ? "closed" : "active",
              Date.now(),
              row.id,
            ),
        ]
      : [];
    steps.push(
      assertion(
        db,
        key,
        "instrument-run-version",
        "EXISTS(SELECT 1 FROM finance_instrument_runs WHERE id=? AND company_id=? AND version=? AND status='draft')",
        [id, actor.companyId, input.expectedVersion],
      ),
      db
        .prepare(
          "UPDATE finance_instrument_runs SET status=?,reviewed_by=?,journal_id=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(
          action === "post" ? "posted" : "rejected",
          actor.userId,
          journal?.result.id ?? null,
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
        status: action === "post" ? "posted" : "rejected",
        version: input.expectedVersion + 1,
        journalId: journal?.result.id ?? null,
      },
      steps,
    );
  });
}
export async function reverseInstrumentRun(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof instrumentReversalSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  const input = instrumentReversalSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = "finance.instrument-run.reverse";
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const run = await getRun(db, actor, id),
      row = await get(db, actor, String(run.instrument_id));
    if (
      run.status !== "posted" ||
      run.version !== input.expectedVersion ||
      !["active", "closed"].includes(String(row.status))
    )
      throw new ApiError(
        409,
        "INSTRUMENT_RUN_CHANGED",
        "Reverse only the latest posted payment in the current agreement edition.",
      );
    if (samePerson(actor, run.prepared_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "An independent Finance approver must reverse the payment.",
      );
    if (
      (await db
        .prepare(
          "SELECT 1 FROM finance_instrument_runs WHERE instrument_id=? AND ((status='posted' AND ordinal>?) OR status='draft')",
        )
        .bind(row.id, run.ordinal)
        .first()) ||
      (await db
        .prepare(
          "SELECT 1 FROM finance_instrument_editions WHERE parent_id=? AND status='draft'",
        )
        .bind(row.id)
        .first())
    )
      throw new ApiError(
        409,
        "INSTRUMENT_LATER_WORK",
        "Decide later work and reverse later payments first.",
      );
    const reversal = await prepareOwnedJournalReversal(
        db,
        actor,
        String(run.journal_id),
        "instrument_payment",
        id,
        input,
        key,
      ),
      now = Date.now();
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
        reversalJournalId: reversal.id,
      },
      [
        ...reversal.steps,
        assertion(
          db,
          key,
          "instrument-bank-owned",
          "EXISTS(SELECT 1 FROM finance_bank_transactions WHERE id=? AND company_id=? AND match_status='matched' AND matched_type='journal' AND matched_id=?)",
          [run.bank_transaction_id, actor.companyId, run.journal_id],
        ),
        db
          .prepare(
            "UPDATE finance_bank_transactions SET match_status='unmatched',matched_type=NULL,matched_id=NULL,version=version+1 WHERE id=?",
          )
          .bind(run.bank_transaction_id),
        db
          .prepare(
            "UPDATE finance_instrument_runs SET status='reversed',reversal_journal_id=?,version=version+1,updated_at=? WHERE id=?",
          )
          .bind(reversal.id, now, id),
        db
          .prepare(
            "UPDATE finance_instrument_editions SET status='active',version=version+1,updated_at=? WHERE id=?",
          )
          .bind(now, row.id),
      ],
    );
  });
}
export async function instrumentWorkspace(actor: WorkContext) {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const [editions, runs, accounts, periods, bank] = await Promise.all([
    write
      .prepare(
        "SELECT * FROM finance_instrument_editions WHERE company_id=? ORDER BY created_at DESC,id LIMIT 500",
      )
      .bind(actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT * FROM finance_instrument_runs WHERE company_id=? ORDER BY created_at DESC,id LIMIT 500",
      )
      .bind(actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT a.id,a.chart_id,a.code,a.name,a.currency,a.category,a.control_kind,c.name chart_name FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.status='active' AND c.status='active' AND NOT EXISTS(SELECT 1 FROM finance_book_rules r WHERE r.company_id=a.company_id AND r.secondary_chart_id=a.chart_id AND r.status='approved') ORDER BY a.chart_id,a.code",
      )
      .bind(actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date",
      )
      .bind(actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT t.id,t.version,t.amount_minor,t.currency,t.transaction_date,t.external_id,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.company_id=? AND t.match_status='unmatched' ORDER BY t.transaction_date DESC LIMIT 500",
      )
      .bind(actor.companyId)
      .all<Row>(),
  ]);
  return {
    editions: editions.results,
    runs: runs.results,
    accounts: accounts.results,
    periods: periods.results,
    bank: bank.results,
  };
}
