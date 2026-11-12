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
  status?: 'draft' | 'posted';
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
  const lines = input.lines.map((line, ordinal) => {
    const debit = minor(line.debit);
    const credit = minor(line.credit);
    if ((debit > 0) === (credit > 0)) throw new Error('SEED_JOURNAL_LINE_INVALID');
    return { accountId: line.accountId, debit, credit, ordinal };
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
        'INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,memo,status,version,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,1000000,\'manual\',?,?,1,?,?,?,?,?)',
      )
      .bind(
        id,
        input.companyId,
        input.chartId,
        input.periodId,
        number,
        input.entryDate,
        input.currency,
        input.memo ?? 'Seeded finance journal for retained fixture setup',
        status,
        input.createdBy,
        status === 'posted' ? input.createdBy : null,
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
          line.debit,
          line.credit,
          line.ordinal,
        ),
    ),
  ];
  await db.batch(statements);
  return { id, number, status, version: 1 };
}
