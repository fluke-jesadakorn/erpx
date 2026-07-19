'use server';

import { revalidatePath } from 'next/cache';
import { query } from '@/db';
import { requireActor, requireAction } from '@/server/guard';

const text = (form: FormData, key: string) => String(form.get(key) ?? '').trim();

export async function saveAccountAction(form: FormData) {
  const actor = await requireActor();
  await requireAction(actor, 'coa_manage', { perm: 'finance:coa:manage::allow' });
  const code = text(form, 'code');
  const accountType = text(form, 'account_type');
  const normalSide = accountType === 'asset' || accountType === 'expense' ? 'debit' : 'credit';
  await query(
    `INSERT INTO finance.accounts(code, name, name_th, account_type, normal_side, control_type)
     VALUES ($1,$2,$3,$4,$5,$6)
     ON CONFLICT (code) DO UPDATE
       SET name = excluded.name, name_th = excluded.name_th,
           account_type = excluded.account_type, normal_side = excluded.normal_side,
           control_type = excluded.control_type, updated_at = now()`,
    [code, text(form, 'name'), text(form, 'name_th') || null, accountType, normalSide, text(form, 'control_type') || null],
  );
  revalidatePath('/ledger');
}

export async function toggleAccountAction(form: FormData) {
  const actor = await requireActor();
  await requireAction(actor, 'coa_manage', { perm: 'finance:coa:manage::allow' });
  await query(`UPDATE finance.accounts SET active = NOT active, updated_at = now() WHERE code = $1`, [text(form, 'code')]);
  revalidatePath('/ledger');
}
