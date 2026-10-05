import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { assertion } from "./commands";
import { ApiError } from "./http";
/** Owner supplies the journal reference; never exposed as a public journal command. */
export async function prepareOwnedJournalReversal(
  db: SqlDatabase,
  actor: WorkContext,
  journalId: string,
  sourceType: "book_adjustment" | "business_recognition" | "instrument_payment" | "asset_policy_depreciation",
  sourceId: string,
  input: { periodId: string; entryDate: string; reason: string },
  key: string,
) {
  const source = await db
    .prepare(
      "SELECT version,entry_date FROM finance_journals WHERE id=? AND company_id=? AND source_type=? AND source_id=? AND status='posted'",
    )
    .bind(journalId, actor.companyId, sourceType, sourceId)
    .first<{ version: number; entry_date: string }>();
  if (!source)
    throw new ApiError(
      409,
      "SOURCE_JOURNAL_CHANGED",
      "The originating journal is unavailable for reversal.",
    );
  if (input.entryDate < source.entry_date)
    throw new ApiError(
      422,
      "REVERSAL_BEFORE_SOURCE",
      "Reverse on or after the original accounting date.",
    );
  const id = `owned-reversal-${key}`,
    now = Date.now();
  return {
    id,
    steps: [
      assertion(
        db,
        key,
        "source-journal",
        "EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND company_id=? AND status='posted' AND version=?)",
        [journalId, actor.companyId, source.version],
      ),
      assertion(
        db,
        key,
        "reversal-period",
        "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)",
        [input.periodId, actor.companyId, input.entryDate, input.entryDate],
      ),
      db
        .prepare(
          "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,?,id,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?",
        )
        .bind(
          id,
          input.periodId,
          `REV-${key}`,
          input.entryDate,
          sourceType === "book_adjustment" ? "book_adjustment" : "reversal",
          input.reason,
          actor.userId,
          actor.userId,
          now,
          now,
          now,
          journalId,
        ),
      db
        .prepare(
          "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?",
        )
        .bind(id, id, journalId),
      db
        .prepare(
          "UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?",
        )
        .bind(actor.userId, now, now, journalId),
    ],
  };
}
