'use server';

import { revalidatePath } from 'next/cache';
import { query, withTransaction } from '@/db';
import { requireActor, requireAction } from '@/server/guard';

const text = (form: FormData, key: string) => String(form.get(key) ?? '').trim();
const number = (form: FormData, key: string) => Number(text(form, key));

export async function setPeriodStatusAction(form: FormData) {
  const actor = await requireActor();
  const periodId = number(form, 'period_id');
  const status = text(form, 'status');
  if (!['open', 'soft_closed', 'locked'].includes(status)) throw new Error('Invalid period status');
  await requireAction(actor, 'period_status', { perm: status === 'open' ? 'finance:period:reopen::allow' : 'finance:period:close::allow' });
  await withTransaction(async (q) => {
    const period = await q<{ status: string }>(`SELECT status FROM finance.fiscal_periods WHERE id = $1 FOR UPDATE`, [periodId]);
    if (!period.rows[0]) throw new Error('Fiscal period not found');
    if (status === 'locked') {
      const pending = await q<{ n: number }>(`SELECT count(*)::int AS n FROM finance.closing_checklists WHERE period_id = $1 AND status = 'pending'`, [periodId]);
      if (pending.rows[0].n > 0) throw new Error('Complete or waive every closing task before hard lock');
    }
    await q(
      `UPDATE finance.fiscal_periods
          SET status = $2,
              closed_by = CASE WHEN $2 <> 'open' THEN $3 ELSE closed_by END,
              closed_at = CASE WHEN $2 <> 'open' THEN now() ELSE closed_at END,
              reopened_by = CASE WHEN $2 = 'open' THEN $3 ELSE reopened_by END,
              reopened_at = CASE WHEN $2 = 'open' THEN now() ELSE reopened_at END
        WHERE id = $1`,
      [periodId, status, actor.id],
    );
  });
  revalidatePath('/accounting');
  revalidatePath('/accounting/periods');
}

export async function updateClosingTaskAction(form: FormData) {
  const actor = await requireActor();
  await requireAction(actor, 'period_close', { perm: 'finance:period:close::allow' });
  const id = number(form, 'task_id');
  const status = text(form, 'status');
  if (!['pending', 'done', 'waived'].includes(status)) throw new Error('Invalid checklist status');
  await query(
    `UPDATE finance.closing_checklists
        SET status = $2,
            completed_by = CASE WHEN $2 = 'pending' THEN NULL ELSE $3 END,
            completed_at = CASE WHEN $2 = 'pending' THEN NULL ELSE now() END
      WHERE id = $1`,
    [id, status, actor.id],
  );
  revalidatePath('/accounting/periods');
}
