'use server';

import { revalidatePath } from 'next/cache';
import { requireActor } from '@/server/guard';
import { submitCapitalContribution, verifyCapitalContribution } from '@/finance/capital';

const text = (form: FormData, key: string) => String(form.get(key) ?? '').trim();
const number = (form: FormData, key: string) => Number(text(form, key));

const capitalActor = async () => {
  const actor = await requireActor();
  return {
    id: actor.id,
    fullname: actor.fullname,
    roleName: actor.role_name,
    departmentId: actor.dept_id,
    permissions: actor.permissions,
  };
};

function refresh() {
  revalidatePath('/capital');
  revalidatePath('/accounting');
  revalidatePath('/ledger');
  revalidatePath('/reports');
  revalidatePath('/executive');
}

export async function submitCapitalContributionAction(form: FormData) {
  const actor = await capitalActor();
  await submitCapitalContribution({
    actor,
    postingDate: text(form, 'posting_date'),
    branchId: number(form, 'branch_id'),
    fundingAccountCode: text(form, 'funding_account_code'),
    equityAccountCode: text(form, 'equity_account_code'),
    reference: text(form, 'reference'),
    note: text(form, 'note'),
    amount: number(form, 'amount'),
    requestKey: text(form, 'request_key'),
  });
  refresh();
}

export async function verifyCapitalContributionAction(form: FormData) {
  const actor = await capitalActor();
  await verifyCapitalContribution(number(form, 'journal_id'), actor);
  refresh();
}
