import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { assertion, executeCommand, replayCommand } from "./commands";
import { financeTransaction } from "./finance-transactions";
import { createJournalSchema, prepareJournal } from "./finance";
import { ApiError } from "./http";
import { sha256 } from "./crypto";
type Row = Record<string, string | number | null>;
export const manualDraftEditSchema = createJournalSchema
  .pick({
    chartId: true,
    periodId: true,
    entryDate: true,
    currency: true,
    memo: true,
    lines: true,
  })
  .extend({
    expectedVersion: z.number().int().positive(),
    reason: z.string().trim().min(10).max(2000),
  })
  .strict();
export async function editManualDraft(
  actor: WorkContext,
  journalId: string,
  raw: z.input<typeof manualDraftEditSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  const input = manualDraftEditSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = "finance.manual-draft.edit";
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, journalId, input);
    if (prior) return prior;
    const old = await db
      .prepare("SELECT * FROM finance_journals WHERE id=? AND company_id=?")
      .bind(journalId, actor.companyId)
      .first<Row>();
    if (!old)
      throw new ApiError(
        404,
        "JOURNAL_NOT_FOUND",
        "Journal not found in this Company.",
      );
    if (
      old.status !== "draft" ||
      old.source_type !== "manual" ||
      old.is_opening ||
      old.source_id ||
      old.version !== input.expectedVersion
    )
      throw new ApiError(
        409,
        "MANUAL_DRAFT_CHANGED",
        "Edit only the current unposted manual draft. Source-owned and posted documents retain their own correction workflow.",
      );
    if (input.chartId !== old.chart_id || input.currency !== old.currency)
      throw new ApiError(
        422,
        "MANUAL_DRAFT_BOOK_CHANGED",
        "Retain this manual draft's book and currency.",
      );
    const lines = (
      await db
        .prepare(
          "SELECT * FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal,id",
        )
        .bind(journalId)
        .all<Row>()
    ).results;
    const prepared = await prepareJournal(
      db,
      actor,
      { ...input, sourceType: "manual", isOpening: false },
      key,
    );
    const now = Date.now(),
      version = input.expectedVersion + 1;
    const next = {
        ...old,
        period_id: input.periodId,
        entry_date: input.entryDate,
        memo: input.memo,
        fx_rate_micros: prepared.result.fxRateMicros,
        created_by: actor.userId,
        version,
        updated_at: now,
      },
      nextLines = prepared.preparedLines.map((l, i) => ({
        id: `${journalId}:v${version}:${i}`,
        journal_id: journalId,
        account_id: l.accountId,
        description: l.description ?? "",
        debit_minor: l.debit,
        credit_minor: l.credit,
        base_debit_minor: l.baseDebit,
        base_credit_minor: l.baseCredit,
        dimension_json: JSON.stringify(l.dimensions ?? {}),
        ordinal: i,
      }));
    const originalSnapshot = JSON.stringify({ journal: old, lines }),
      snapshot = JSON.stringify({ journal: next, lines: nextLines });
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      journalId,
      version,
      { id: journalId, status: "draft", version },
      [
        assertion(
          db,
          key,
          "manual-draft-version",
          "EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND company_id=? AND source_type='manual' AND status='draft' AND version=?)",
          [journalId, actor.companyId, input.expectedVersion],
        ),
        ...prepared.guards,
        db
          .prepare(
            "INSERT INTO finance_manual_draft_editions(id,company_id,journal_id,journal_version,snapshot_json,snapshot_sha256,reason,prepared_by,created_at) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(journal_id,journal_version) DO NOTHING",
          )
          .bind(
            `${journalId}:v${old.version}`,
            actor.companyId,
            journalId,
            old.version,
            originalSnapshot,
            await sha256(originalSnapshot),
            "Retained original draft before the first edit",
            old.created_by,
            now,
          ),
        db
          .prepare("DELETE FROM finance_journal_lines WHERE journal_id=?")
          .bind(journalId),
        db
          .prepare(
            "UPDATE finance_journals SET period_id=?,entry_date=?,memo=?,fx_rate_micros=?,created_by=?,version=?,updated_at=? WHERE id=?",
          )
          .bind(
            input.periodId,
            input.entryDate,
            input.memo,
            prepared.result.fxRateMicros,
            actor.userId,
            version,
            now,
            journalId,
          ),
        ...nextLines.map((l) =>
          db
            .prepare(
              "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,?,?,?,?,?,?,?)",
            )
            .bind(
              l.id,
              l.journal_id,
              l.account_id,
              l.description,
              l.debit_minor,
              l.credit_minor,
              l.base_debit_minor,
              l.base_credit_minor,
              l.dimension_json,
              l.ordinal,
            ),
        ),
        db
          .prepare(
            "INSERT INTO finance_manual_draft_editions(id,company_id,journal_id,journal_version,snapshot_json,snapshot_sha256,reason,prepared_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            `${journalId}:v${version}`,
            actor.companyId,
            journalId,
            version,
            snapshot,
            await sha256(snapshot),
            input.reason,
            actor.userId,
            now,
          ),
      ],
    );
  });
}
