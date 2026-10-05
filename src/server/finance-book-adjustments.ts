import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { executeCommand, replayCommand, assertion } from "./commands";
import { financeTransaction } from "./finance-transactions";
import { prepareJournal, createJournalSchema } from "./finance";
import { prepareOwnedJournalReversal } from "./finance-owned-reversal";
import { financeReviewSchema } from "./finance-books";
import { samePerson } from "@/lib/iam";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
export const bookAdjustmentSchema = createJournalSchema
  .pick({ chartId: true, periodId: true, entryDate: true, lines: true })
  .extend({
    primaryJournalId: z.string().min(1),
    reason: z.string().trim().min(3).max(2000),
  });
export const bookAdjustmentReversalSchema = financeReviewSchema.extend({
  periodId: z.string().min(1),
  entryDate: z.string().date(),
});
type Row = Record<string, string | number | null>;
async function evidence(
  db: SqlDatabase,
  actor: WorkContext,
  input: z.infer<typeof bookAdjustmentSchema>,
) {
  const source = await db
    .prepare(
      "SELECT p.id,p.version,p.status,p.entry_date,p.chart_id,s.base_currency FROM finance_journals p JOIN finance_book_postings b ON b.primary_journal_id=p.id JOIN finance_journals j ON j.id=b.secondary_journal_id JOIN finance_charts s ON s.id=j.chart_id AND s.status='active' WHERE p.id=? AND p.company_id=? AND j.chart_id=? AND p.status='posted' AND j.status='posted'",
    )
    .bind(input.primaryJournalId, actor.companyId, input.chartId)
    .first<Row>();
  if (!source)
    throw new ApiError(
      409,
      "BOOK_SOURCE_REQUIRED",
      "Choose a currently posted source projected into this secondary book.",
    );
  if (input.entryDate < String(source.entry_date))
    throw new ApiError(
      422,
      "BOOK_ADJUSTMENT_DATE",
      "Adjust on or after the source posting date.",
    );
  const ids = [...new Set(input.lines.map((l) => l.accountId))],
    accounts = (
      await db
        .prepare(
          `SELECT id,version,category,currency FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND is_control=0 AND control_kind='none' AND id IN (${ids.map(() => "?").join(",")}) ORDER BY id`,
        )
        .bind(actor.companyId, input.chartId, ...ids)
        .all<Row>()
    ).results;
  if (
    accounts.length !== ids.length ||
    accounts.some((a) => a.currency !== source.base_currency)
  )
    throw new ApiError(
      422,
      "BOOK_ADJUSTMENT_ACCOUNT_INVALID",
      "Use active noncontrol accounts in the secondary functional currency. Control differences require a dedicated subledger plan.",
    );
  return {
    source,
    accounts,
    hash: await sha256(JSON.stringify({ input, source, accounts })),
  };
}
export async function prepareBookAdjustment(
  actor: WorkContext,
  raw: z.input<typeof bookAdjustmentSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  const input = bookAdjustmentSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    id = `book-adjustment-${key}`,
    op = "finance.book-adjustment.prepare";
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const source = await evidence(db, actor, input);
    // Preparation validates balance, period and money evidence without persisting a journal.
    await prepareJournal(
      db,
      actor,
      {
        ...input,
        currency: String(source.source.base_currency),
        sourceType: "book_adjustment",
        sourceId: id,
        memo: input.reason,
        isOpening: false,
      },
      key,
    );
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      1,
      { id, status: "draft", version: 1, sourceHash: source.hash },
      [
        db
          .prepare(
            "INSERT INTO finance_book_adjustments(id,company_id,chart_id,primary_journal_id,source_version,content_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            actor.companyId,
            input.chartId,
            input.primaryJournalId,
            source.source.version,
            JSON.stringify(input),
            source.hash,
            actor.userId,
            input.reason,
            Date.now(),
            Date.now(),
          ),
      ],
    );
  });
}
export async function decideBookAdjustment(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof financeReviewSchema>,
  action: "post" | "reject",
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  const input = financeReviewSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = `finance.book-adjustment.${action}`;
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const row = await db
      .prepare(
        "SELECT * FROM finance_book_adjustments WHERE id=? AND company_id=?",
      )
      .bind(id, actor.companyId)
      .first<Row>();
    if (!row)
      throw new ApiError(
        404,
        "BOOK_ADJUSTMENT_NOT_FOUND",
        "Book adjustment not found.",
      );
    if (row.status !== "draft" || row.version !== input.expectedVersion)
      throw new ApiError(
        409,
        "BOOK_ADJUSTMENT_CHANGED",
        "Refresh the draft before deciding.",
      );
    if (samePerson(actor, row.prepared_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must decide the book adjustment.",
      );
    const content = bookAdjustmentSchema.parse(
        JSON.parse(String(row.content_json)),
      ),
      source = action === "post" ? await evidence(db, actor, content) : null;
    if (source && source.hash !== row.source_hash)
      throw new ApiError(
        409,
        "BOOK_ADJUSTMENT_SOURCE_CHANGED",
        "Source or account evidence changed. Reject and prepare a fresh adjustment.",
      );
    const journal = source
        ? await prepareJournal(
            db,
            actor,
            {
              ...content,
              currency: String(source.source.base_currency),
              sourceType: "book_adjustment",
              sourceId: id,
              memo: content.reason,
              isOpening: false,
            },
            key,
            "posted",
          )
        : null,
      status = action === "post" ? "posted" : "rejected",
      version = input.expectedVersion + 1;
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      version,
      { id, status, version, journalId: journal?.result.id ?? null },
      [
        assertion(
          db,
          key,
          "adjustment-version",
          "EXISTS(SELECT 1 FROM finance_book_adjustments WHERE id=? AND company_id=? AND status='draft' AND version=?)",
          [id, actor.companyId, input.expectedVersion],
        ),
        ...(journal?.steps ?? []),
        db
          .prepare(
            "UPDATE finance_book_adjustments SET status=?,version=?,reviewed_by=?,journal_id=?,updated_at=? WHERE id=?",
          )
          .bind(
            status,
            version,
            actor.userId,
            journal?.result.id ?? null,
            Date.now(),
            id,
          ),
        db
          .prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)")
          .bind(
            crypto.randomUUID(),
            key,
            id,
            op,
            actor.userId,
            actor.assignmentId,
            input.expectedVersion,
            await sha256(JSON.stringify(input)),
            Date.now(),
          ),
      ],
    );
  });
}
export async function reverseBookAdjustment(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof bookAdjustmentReversalSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  const input = bookAdjustmentReversalSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = "finance.book-adjustment.reverse";
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const row = await db
      .prepare(
        "SELECT * FROM finance_book_adjustments WHERE id=? AND company_id=?",
      )
      .bind(id, actor.companyId)
      .first<Row>();
    if (!row)
      throw new ApiError(
        404,
        "BOOK_ADJUSTMENT_NOT_FOUND",
        "Book adjustment not found.",
      );
    if (row.status !== "posted" || row.version !== input.expectedVersion)
      throw new ApiError(
        409,
        "BOOK_ADJUSTMENT_CHANGED",
        "Refresh the posted adjustment before reversing it.",
      );
    if (samePerson(actor, row.prepared_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must reverse the book adjustment.",
      );
    const reversal = await prepareOwnedJournalReversal(
      db,
      actor,
      String(row.journal_id),
      "book_adjustment",
      id,
      input,
      key,
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
        status: "reversed",
        version: input.expectedVersion + 1,
        reversalJournalId: reversal.id,
      },
      [
        assertion(
          db,
          key,
          "adjustment-version",
          "EXISTS(SELECT 1 FROM finance_book_adjustments WHERE id=? AND company_id=? AND status='posted' AND version=?)",
          [id, actor.companyId, input.expectedVersion],
        ),
        ...reversal.steps,
        db
          .prepare(
            "UPDATE finance_book_adjustments SET status='reversed',version=version+1,reversal_journal_id=?,reason=?,updated_at=? WHERE id=?",
          )
          .bind(reversal.id, input.reason, Date.now(), id),
      ],
    );
  });
}
export async function bookAdjustmentWorkspace(actor: WorkContext) {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  return (
    await write
      .prepare(
        "SELECT * FROM finance_book_adjustments WHERE company_id=? ORDER BY created_at DESC LIMIT 200",
      )
      .bind(actor.companyId)
      .all<Row>()
  ).results;
}
export async function bookAdjustmentConsoleWorkspace(actor: WorkContext) {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const [rows, books, accounts, sources, periods] = await Promise.all([
    bookAdjustmentWorkspace(actor),
    write
      .prepare(
        "SELECT DISTINCT c.id,c.name,c.base_currency FROM finance_charts c JOIN finance_book_rules r ON r.secondary_chart_id=c.id WHERE c.company_id=? AND c.status='active' AND r.status='approved' ORDER BY c.name",
      )
      .bind(actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT id,chart_id,code,name FROM finance_accounts WHERE company_id=? AND status='active' AND is_control=0 AND control_kind='none' ORDER BY code",
      )
      .bind(actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT p.id,p.journal_number,p.entry_date,p.memo,s.chart_id FROM finance_book_postings b JOIN finance_journals p ON p.id=b.primary_journal_id JOIN finance_journals s ON s.id=b.secondary_journal_id WHERE b.company_id=? AND p.status='posted' AND s.status='posted' ORDER BY p.entry_date DESC,p.id LIMIT 500",
      )
      .bind(actor.companyId)
      .all<Row>(),
    write
      .prepare(
        "SELECT id,name FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC",
      )
      .bind(actor.companyId)
      .all<Row>(),
  ]);
  return {
    rows,
    books: books.results,
    accounts: accounts.results,
    sources: sources.results,
    periods: periods.results,
  };
}
