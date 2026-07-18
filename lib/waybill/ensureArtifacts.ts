import 'server-only';
import type { query as DbQuery } from '../db';
import { upsertDraftJournal, finalizeDraftJournal } from '../finance/postExpenseToGL';
import { PETTY_THRESHOLD_THB } from './accountFlow';

type Client = typeof DbQuery;

interface ExpenseRow {
  pr_id: number | null;
  po_id: number | null;
  journal_entry_id: number | null;
  total_amount: string | null;
  vendor_name: string | null;
  submitter_id: number | null;
}

async function loadExpense(client: Client, expenseId: number): Promise<ExpenseRow | null> {
  const r = await client<ExpenseRow>(
    `SELECT pr_id, po_id, journal_entry_id, total_amount, vendor_name, submitter_id
       FROM expenses WHERE id = $1`,
    [expenseId],
  );
  return r.rows[0] ?? null;
}

export async function ensurePrForExpense(
  client: Client,
  expenseId: number,
): Promise<number | null> {
  const exp = await loadExpense(client, expenseId);
  if (!exp) return null;
  if (exp.pr_id) return exp.pr_id;

  const total = exp.total_amount ? parseFloat(exp.total_amount) : 0;
  if (total < PETTY_THRESHOLD_THB) return null;

  const ins = await client<{ id: number }>(
    `INSERT INTO purchase_requisitions
        (requester_id, vendor_name, total_estimate, currency, status, dept_group_id)
     VALUES ($1, $2, $3, 'THB', 'submission',
             (SELECT department_id FROM perm.user_departments WHERE user_id = $1))
     RETURNING id`,
    [exp.submitter_id, exp.vendor_name ?? '', total],
  );
  const prId = ins.rows[0].id;

  await client(
    `UPDATE expenses SET pr_id = $1, updated_at = now() WHERE id = $2`,
    [prId, expenseId],
  );
  return prId;
}

export async function ensurePoForExpense(
  client: Client,
  expenseId: number,
): Promise<number | null> {
  const exp = await loadExpense(client, expenseId);
  if (!exp) return null;
  if (exp.po_id) return exp.po_id;

  const prId = exp.pr_id;
  if (!prId) return null;

  const prRow = await client<{ total_estimate: string | null; currency: string | null }>(
    `SELECT total_estimate, currency FROM purchase_requisitions WHERE id = $1`,
    [prId],
  );
  const pr = prRow.rows[0];
  if (!pr) return null;

  const total = pr.total_estimate ? parseFloat(pr.total_estimate) : 0;

  const expCreated = await client<{ created_at: Date }>(
    `SELECT created_at FROM purchase_requisitions WHERE id = $1`,
    [prId],
  );
  const fiscalYear = expCreated.rows[0]
    ? new Date(expCreated.rows[0].created_at).getFullYear()
    : new Date().getFullYear();

  const poNum = await client<{ po_number: string }>(
    `SELECT next_purchase_order_number($1) AS po_number`,
    [fiscalYear],
  );
  const poNumber = poNum.rows[0].po_number;

  const ins = await client<{ id: number }>(
    `INSERT INTO purchase_orders
        (pr_id, po_number, vendor_name, total_amount, currency, status, issued_by)
     VALUES ($1, $2, $3, $4, $5, 'accounting_authorization', $6)
     RETURNING id`,
    [prId, poNumber, exp.vendor_name ?? '', total, pr.currency ?? 'THB', exp.submitter_id],
  );
  const poId = ins.rows[0].id;

  await client(
    `UPDATE expenses SET po_id = $1, updated_at = now() WHERE id = $2`,
    [poId, expenseId],
  );
  return poId;
}

export async function ensureGlForExpense(
  client: Client,
  expenseId: number,
  actorId: number,
): Promise<number | null> {
  const exp = await loadExpense(client, expenseId);
  if (!exp) return null;
  if (exp.journal_entry_id) return exp.journal_entry_id;

  await upsertDraftJournal({
    expenseId,
    vendorName: exp.vendor_name ?? '',
  });
  const finalized = await finalizeDraftJournal({ expenseId, actorId });
  if (!finalized) return null;

  await client(
    `UPDATE expenses
        SET journal_entry_id = $1,
            updated_at = now()
      WHERE id = $2`,
    [finalized.journalId, expenseId],
  );
  return finalized.journalId;
}
