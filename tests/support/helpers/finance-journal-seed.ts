import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

/**
 * SQL-only seeding for finance journals in fixtures and tests.
 *
 * The journal create/post writers are being retired in favour of the native
 * journal commands; fixtures that only need an existing draft or posted journal
 * use this helper instead of importing the TypeScript writer. Amounts are
 * decimal strings with at most two fraction digits, matching the retained
 * fixture inputs. Journals are seeded manual (no source), balanced, with a
 * 1:1 functional rate.
 */
export type SeedFinanceJournalLine = Readonly<{ accountId: string; debit: string; credit: string }>;

export type SeedFinanceJournalInput = Readonly<{
  companyId: string;
  chartId: string;
  periodId: string;
  entryDate: string;
  currency: string;
  memo?: string;
  key: string;
  createdBy: string;
  /** Independent reviewer for posted journals that enforce same-person checks. */
  postedBy?: string;
  status?: 'draft' | 'posted';
  /** Retained source label for fixtures that prove owner-only source authority. */
  sourceType?: string;
  sourceId?: string;
  isOpening?: boolean;
  /** One million means 1:1; retained FX fixtures pass the reviewed rate. */
  fxRateMicros?: number;
  lines: readonly SeedFinanceJournalLine[];
}>;

const minor = (value: string): number => {
  if (!/^\d+(\.\d{1,2})?$/.test(value)) throw new Error('SEED_JOURNAL_AMOUNT_INVALID');
  const [whole, fraction = ''] = value.split('.');
  return Number(whole) * 100 + Number(fraction.padEnd(2, '0'));
};

export async function seedFinanceJournal(
  db: SqlDatabase,
  input: SeedFinanceJournalInput,
): Promise<{ id: string; number: string; status: 'draft' | 'posted'; version: number }> {
  const status = input.status ?? 'draft';
  const fxRateMicros = input.fxRateMicros ?? 1_000_000;
  const lines = input.lines.map((line, ordinal) => {
    const debit = minor(line.debit);
    const credit = minor(line.credit);
    if ((debit > 0) === (credit > 0)) throw new Error('SEED_JOURNAL_LINE_INVALID');
    return { accountId: line.accountId, debit, credit, baseDebit: Math.round((debit * fxRateMicros) / 1_000_000), baseCredit: Math.round((credit * fxRateMicros) / 1_000_000), ordinal };
  });
  const debitTotal = lines.reduce((sum, line) => sum + line.debit, 0);
  const creditTotal = lines.reduce((sum, line) => sum + line.credit, 0);
  if (debitTotal !== creditTotal || debitTotal === 0) throw new Error('SEED_JOURNAL_UNBALANCED');
  const id = `journal-seed-${input.key}`;
  const number = `J-SEED-${input.key}`;
  const now = Date.now();
  const statements = [
    db
      .prepare(
        'INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,is_opening,status,version,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,1,?,?,?,?,?)',
      )
      .bind(
        id,
        input.companyId,
        input.chartId,
        input.periodId,
        number,
        input.entryDate,
        input.currency,
        fxRateMicros,
        input.sourceType ?? 'manual',
        input.sourceId ?? null,
        input.memo ?? 'Seeded finance journal for retained fixture setup',
        input.isOpening ? 1 : 0,
        status,
        input.createdBy,
        status === 'posted' ? (input.postedBy ?? input.createdBy) : null,
        status === 'posted' ? now : null,
        now,
        now,
      ),
    ...lines.map((line) =>
      db
        .prepare(
          'INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES(?,?,?,?,?,?,?,?)',
        )
        .bind(
          `${id}-${line.ordinal}`,
          id,
          line.accountId,
          line.debit,
          line.credit,
          line.baseDebit,
          line.baseCredit,
          line.ordinal,
        ),
    ),
  ];
  await db.batch(statements);
  return { id, number, status, version: 1 };
}

/**
 * Fixture setup for journals produced by native owner writes (goods receipts,
 * landed cost, Supply AP). The TS journal post writer is retired; these tests
 * only need the already-reviewed native draft to be in its posted state.
 */
export async function markFinanceJournalPosted(
  db: SqlDatabase,
  journalId: string,
  postedBy: string,
): Promise<void> {
  const now = Date.now();
  await db
    .prepare(
      "UPDATE finance_journals SET status='posted',version=version+1,posted_by=?,posted_at=?,updated_at=? WHERE id=? AND status IN ('draft','in_review')",
    )
    .bind(postedBy, now, now, journalId)
    .run();
  await db
    .prepare(
      "UPDATE finance_invoices SET status='posted',approved_by=?,posted_at=?,version=version+1,updated_at=? WHERE source_type='opening_import' AND source_id=? AND status='draft'",
    )
    .bind(postedBy, now, now, journalId)
    .run();
}

/** SQL-only reversal of a seeded posted journal for retained report setup. */
export async function seedFinanceJournalReversal(
  db: SqlDatabase,
  input: Readonly<{
    journalId: string;
    periodId: string;
    entryDate: string;
    memo?: string;
    key: string;
    reversedBy: string;
  }>,
): Promise<string> {
  const now = Date.now();
  const reversalId = `reversal-seed-${input.key}`;
  await db.batch([
    db
      .prepare(
        "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,version,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'reversal',id,?,'posted',1,?,?,?,?,? FROM finance_journals WHERE id=?",
      )
      .bind(
        reversalId,
        input.periodId,
        `REV-SEED-${input.key}`,
        input.entryDate,
        input.memo ?? 'Seeded reversal for retained fixture setup',
        input.reversedBy,
        input.reversedBy,
        now,
        now,
        now,
        input.journalId,
      ),
    db
      .prepare(
        "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?",
      )
      .bind(reversalId, reversalId, input.journalId),
    db
      .prepare(
        "UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=? AND status='posted'",
      )
      .bind(input.reversedBy, now, now, input.journalId),
  ]);
  return reversalId;
}
