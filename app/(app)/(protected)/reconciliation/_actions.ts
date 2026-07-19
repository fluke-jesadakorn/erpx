'use server';

import { revalidatePath } from 'next/cache';
import { query, withTransaction } from '@/db';
import { requireActor, requireAction } from '@/server/guard';

const text = (form: FormData, key: string) => String(form.get(key) ?? '').trim();
const number = (form: FormData, key: string) => Number(text(form, key));

export async function createBankAccountAction(form: FormData) {
  const actor = await requireActor();
  await requireAction(actor, 'bank_account_create', { perm: 'finance:bank:import::allow' });
  await query(
    `INSERT INTO finance.bank_accounts
       (branch_id, code, bank_name, account_name, account_number_masked, currency_code, gl_account_code)
     VALUES ($1,$2,$3,$4,$5,$6,$7)`,
    [number(form, 'branch_id'), text(form, 'code'), text(form, 'bank_name'), text(form, 'account_name'), text(form, 'account_number_masked'), text(form, 'currency_code'), text(form, 'gl_account_code')],
  );
  revalidatePath('/reconciliation');
}

function ids(value: string) {
  const list = value.split(',').map((item) => Number(item.trim())).filter((item) => Number.isInteger(item) && item > 0);
  if (!list.length) throw new Error('At least one ID is required');
  return [...new Set(list)];
}

export async function confirmBankMatchAction(form: FormData) {
  const actor = await requireActor();
  await requireAction(actor, 'bank_match', { perm: 'finance:bank:match::allow' });
  const transactionIds = ids(text(form, 'bank_transaction_ids'));
  const journalIds = ids(text(form, 'journal_ids'));
  const fee = Math.round(number(form, 'fee_thb') * 100) / 100 || 0;
  const fx = Math.round(number(form, 'fx_difference_thb') * 100) / 100 || 0;
  await withTransaction(async (q) => {
    const transactions = await q<{ id: string; bank_account_id: string; amount: string; status: string }>(
      `SELECT id::text, bank_account_id::text, amount::text, status
         FROM finance.bank_transactions WHERE id = ANY($1::bigint[]) FOR UPDATE`,
      [transactionIds],
    );
    if (transactions.rows.length !== transactionIds.length || transactions.rows.some((row) => row.status === 'matched')) throw new Error('One or more bank transactions are unavailable');
    const accountIds = new Set(transactions.rows.map((row) => row.bank_account_id));
    if (accountIds.size !== 1) throw new Error('A match group must use one bank account');
    const lockedJournals = await q<{ id: string }>(
      `SELECT id::text FROM finance.journals WHERE id = ANY($1::bigint[]) AND status = 'posted' FOR UPDATE`,
      [journalIds],
    );
    if (lockedJournals.rows.length !== journalIds.length) throw new Error('Every target journal must be posted');
    const journals = await q<{ id: string; total: string }>(
      `SELECT j.id::text, sum(l.debit_thb)::text AS total
         FROM finance.journals j JOIN finance.journal_lines l ON l.journal_id = j.id
        WHERE j.id = ANY($1::bigint[]) AND j.status = 'posted'
        GROUP BY j.id`,
      [journalIds],
    );
    const bankTotal = transactions.rows.reduce((sum, row) => sum + Math.abs(Number(row.amount)), 0);
    const targetTotal = journals.rows.reduce((sum, row) => sum + Number(row.total), 0);
    const difference = Math.round((bankTotal - targetTotal) * 100) / 100;
    if (Math.abs(difference - fee - fx) > 0.01) throw new Error(`Unresolved match difference: THB ${difference.toFixed(2)}`);
    const group = await q<{ id: string }>(
      `INSERT INTO finance.bank_match_groups
         (bank_account_id, difference_thb, fee_thb, fx_difference_thb, confirmed_by)
       VALUES ($1,$2,$3,$4,$5) RETURNING id::text`,
      [Number(transactions.rows[0].bank_account_id), difference, fee, fx, actor.id],
    );
    const groupId = Number(group.rows[0].id);
    for (const row of transactions.rows) await q(`INSERT INTO finance.bank_match_lines(group_id, bank_transaction_id, amount_thb) VALUES ($1,$2,$3)`, [groupId, Number(row.id), Math.abs(Number(row.amount))]);
    for (const row of journals.rows) await q(`INSERT INTO finance.bank_match_lines(group_id, journal_id, amount_thb) VALUES ($1,$2,$3)`, [groupId, Number(row.id), Number(row.total)]);
    await q(`UPDATE finance.bank_transactions SET status = 'matched' WHERE id = ANY($1::bigint[])`, [transactionIds]);
    await q(`INSERT INTO finance.bank_reconciliation_audit(match_group_id, action, actor_id, payload) VALUES ($1,'confirmed',$2,$3)`, [groupId, actor.id, { transactionIds, journalIds, difference, fee, fx }]);
  });
  revalidatePath('/reconciliation');
  revalidatePath('/executive');
}

export async function reopenBankMatchAction(form: FormData) {
  const actor = await requireActor();
  await requireAction(actor, 'bank_reopen', { perm: 'finance:bank:reopen::allow' });
  const groupId = number(form, 'group_id');
  const reason = text(form, 'reason');
  if (!reason) throw new Error('A reopen reason is required');
  await withTransaction(async (q) => {
    const group = await q<{ status: string }>(`SELECT status FROM finance.bank_match_groups WHERE id = $1 FOR UPDATE`, [groupId]);
    if (!group.rows[0] || group.rows[0].status !== 'confirmed') throw new Error('Match group is not confirmed');
    await q(`UPDATE finance.bank_match_groups SET status = 'reopened', reopened_by = $2, reopened_at = now(), reopen_reason = $3 WHERE id = $1`, [groupId, actor.id, reason]);
    await q(`UPDATE finance.bank_transactions SET status = 'reopened' WHERE id IN (SELECT bank_transaction_id FROM finance.bank_match_lines WHERE group_id = $1 AND bank_transaction_id IS NOT NULL)`, [groupId]);
    await q(`INSERT INTO finance.bank_reconciliation_audit(match_group_id, action, actor_id, reason) VALUES ($1,'reopened',$2,$3)`, [groupId, actor.id, reason]);
  });
  revalidatePath('/reconciliation');
  revalidatePath('/executive');
}
