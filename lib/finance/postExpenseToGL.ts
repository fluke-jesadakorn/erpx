import 'server-only';
import { query, withTransaction } from '../db';

export interface GlPostResult {
  journalId: number;
}

export interface DraftGlLine {
  account_code: string;
  account_name?: string | null;
  account_name_th?: string | null;
  debit: number;
  credit: number;
  description: string;
}

async function buildGlLines(expenseId: number): Promise<DraftGlLine[]> {
  const expRes = await query<{ vat_amount: string | null; total_amount: string | null }>(
    `SELECT vat_amount, total_amount FROM expenses WHERE id = $1`,
    [expenseId],
  );
  const vat = expRes.rows[0]?.vat_amount ? parseFloat(String(expRes.rows[0].vat_amount)) : 0;
  const total = expRes.rows[0]?.total_amount ? parseFloat(String(expRes.rows[0].total_amount)) : 0;

  const itemsRes = await query<{
    amount: string;
    mapped_account_code: string | null;
    description: string | null;
    account_name: string | null;
    account_name_th: string | null;
  }>(
    `SELECT i.amount, i.mapped_account_code, i.description,
            c.name AS account_name, c.name_th AS account_name_th
       FROM expense_items i
       LEFT JOIN chart_of_accounts c ON c.code = i.mapped_account_code
      WHERE i.expense_id = $1
   ORDER BY i.id ASC`,
    [expenseId],
  );

  const accountsRes = await query<{ code: string; name: string | null; name_th: string | null }>(
    `SELECT code, name, name_th FROM chart_of_accounts WHERE code = ANY($1::text[])`,
    [['110500', '110200']],
  );
  const acctMap = new Map(accountsRes.rows.map((a) => [a.code, a]));

  const lines: DraftGlLine[] = [];
  for (const item of itemsRes.rows) {
    lines.push({
      account_code: item.mapped_account_code || '510300',
      account_name: item.account_name,
      account_name_th: item.account_name_th,
      debit: parseFloat(String(item.amount)),
      credit: 0,
      description: item.description ?? '',
    });
  }

  if (vat > 0) {
    const a = acctMap.get('110500');
    lines.push({
      account_code: '110500',
      account_name: a?.name ?? null,
      account_name_th: a?.name_th ?? null,
      debit: vat,
      credit: 0,
      description: `Input VAT 7% for EXP-${expenseId}`,
    });
  }

  if (total > 0) {
    const a = acctMap.get('110200');
    lines.push({
      account_code: '110200',
      account_name: a?.name ?? null,
      account_name_th: a?.name_th ?? null,
      debit: 0,
      credit: total,
      description: `Disbursed cash at bank for EXP-${expenseId}`,
    });
  }

  return lines;
}

export async function postExpenseToGL(args: {
  expenseId: number;
  vendorName: string;
  totalAmount: number | string;
  vatAmount: number | string;
}): Promise<GlPostResult> {
  void args.totalAmount;
  void args.vatAmount;
  const lines = await buildGlLines(args.expenseId);

  const journalRes = await query<{ id: number }>(
    `INSERT INTO journal_entries (expense_id, description)
     VALUES ($1, $2) RETURNING id`,
    [args.expenseId, `Disbursed employee reimbursement for ${args.vendorName} (EXP-${args.expenseId})`],
  );
  const journalId: number = journalRes.rows[0].id;

  for (const ln of lines) {
    await query(
      `INSERT INTO ledger_lines (journal_entry_id, account_code, debit, credit, description)
       VALUES ($1, $2, $3, $4, $5)`,
      [journalId, ln.account_code, ln.debit, ln.credit, ln.description],
    );
  }

  return { journalId };
}

export async function upsertDraftJournal(args: {
  expenseId: number;
  vendorName: string;
}): Promise<{ journalId: number; lines: DraftGlLine[] }> {
  const lines = await buildGlLines(args.expenseId);
  const journalId = await withTransaction(async (q) => {
    const existing = await q<{ id: number }>(
      `SELECT id FROM journal_entries
        WHERE expense_id = $1 AND is_draft = TRUE
        LIMIT 1`,
      [args.expenseId],
    );
    let id: number;
    if (existing.rows[0]) {
      id = existing.rows[0].id;
    } else {
      const desc = `DRAFT: Disbursed reimbursement for ${args.vendorName} (EXP-${args.expenseId})`;
      const ins = await q<{ id: number }>(
        `INSERT INTO journal_entries (expense_id, description, is_draft, draft_source)
         VALUES ($1, $2, TRUE, 'expense')
         RETURNING id`,
        [args.expenseId, desc],
      );
      id = ins.rows[0].id;
    }
    await q(`DELETE FROM ledger_lines WHERE journal_entry_id = $1`, [id]);
    for (const ln of lines) {
      await q(
        `INSERT INTO ledger_lines (journal_entry_id, account_code, debit, credit, description)
         VALUES ($1, $2, $3, $4, $5)`,
        [id, ln.account_code, ln.debit, ln.credit, ln.description],
      );
    }
    return id;
  });
  return { journalId, lines };
}

export async function recomputeDraftJournal(args: {
  expenseId: number;
  vendorName: string;
}): Promise<{ journalId: number; lines: DraftGlLine[] } | null> {
  const draft = await query<{ id: number }>(
    `SELECT id FROM journal_entries WHERE expense_id = $1 AND is_draft = TRUE LIMIT 1`,
    [args.expenseId],
  );
  if (!draft.rows[0]) return null;
  return upsertDraftJournal(args);
}

export async function finalizeDraftJournal(args: {
  expenseId: number;
  actorId: number;
}): Promise<{ journalId: number } | null> {
  return withTransaction(async (q) => {
    const r = await q<{ id: number }>(
      `SELECT id FROM journal_entries
        WHERE expense_id = $1 AND is_draft = TRUE
        LIMIT 1`,
      [args.expenseId],
    );
    const draft = r.rows[0];
    if (!draft) return null;
    await q(
      `UPDATE journal_entries
          SET is_draft = FALSE,
              finalized_at = now(),
              finalized_by = $1
        WHERE id = $2`,
      [args.actorId, draft.id],
    );
    return { journalId: draft.id };
  });
}

export async function setExpenseJournalEntry(
  client: typeof import('../db').query,
  expenseId: number,
  journalId: number,
  actorId: number,
): Promise<void> {
  await client(
    `UPDATE expenses
       SET journal_entry_id = $1,
           updated_at = now()
     WHERE id = $3`,
    [journalId, actorId, expenseId],
  );
}