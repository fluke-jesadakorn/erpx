'use server';

import { revalidatePath } from 'next/cache';
import { query, withTransaction } from '@/db';
import { requireActor, requireAction } from '@/server/guard';

const text = (form: FormData, key: string) => String(form.get(key) ?? '').trim();
const number = (form: FormData, key: string) => Number(text(form, key));

export async function createBudgetAction(form: FormData) {
  const actor = await requireActor();
  await requireAction(actor, 'budget_manage', { perm: 'finance:budget:manage::allow' });
  await query(
    `INSERT INTO finance.budgets(name, fiscal_year, branch_id, created_by)
     VALUES ($1,$2,$3,$4)`,
    [text(form, 'name'), number(form, 'fiscal_year'), number(form, 'branch_id') || null, actor.id],
  );
  revalidatePath('/budgets');
}

export async function saveBudgetLineAction(form: FormData) {
  const actor = await requireActor();
  await requireAction(actor, 'budget_manage', { perm: 'finance:budget:manage::allow' });
  await withTransaction(async (q) => {
    const budgetId = number(form, 'budget_id');
    const budget = await q<{ status: string }>(`SELECT status FROM finance.budgets WHERE id = $1 FOR UPDATE`, [budgetId]);
    if (!budget.rows[0] || budget.rows[0].status !== 'draft') throw new Error('Only draft budgets can be edited');
    await q(
      `INSERT INTO finance.budget_lines(budget_id, period_no, account_code, department_id, amount_thb)
       VALUES ($1,$2,$3,$4,$5)
       ON CONFLICT (budget_id, period_no, account_code, department_id) DO UPDATE SET amount_thb = excluded.amount_thb`,
      [budgetId, number(form, 'period_no'), text(form, 'account_code'), text(form, 'department_id') || null, number(form, 'amount_thb')],
    );
  });
  revalidatePath('/budgets');
  revalidatePath('/reports');
}

export async function approveBudgetAction(form: FormData) {
  const actor = await requireActor();
  await requireAction(actor, 'budget_approve', { perm: 'finance:budget:manage::allow' });
  await query(`UPDATE finance.budgets SET status = 'approved', approved_by = $2, approved_at = now() WHERE id = $1 AND status = 'draft'`, [number(form, 'budget_id'), actor.id]);
  revalidatePath('/budgets');
  revalidatePath('/reports');
  revalidatePath('/executive');
}
