'use server';

import { revalidatePath } from 'next/cache';
import { requireActor } from '@/server/guard';
import { approveAndPostJournal, postManualJournal, reverseJournal, voidJournal } from '@/finance';

const text = (form: FormData, key: string) => String(form.get(key) ?? '').trim();
const number = (form: FormData, key: string) => Number(text(form, key));

export async function postPreparedAction(form: FormData) {
  const actor = await requireActor();
  await approveAndPostJournal(number(form, 'journal_id'), { id: actor.id, permissions: actor.permissions });
  revalidatePath('/accounting');
  revalidatePath('/ledger');
  revalidatePath('/executive');
}

export async function createManualJournalAction(form: FormData) {
  const actor = await requireActor();
  const date = text(form, 'posting_date');
  const amount = number(form, 'amount');
  const description = text(form, 'description');
  const branchId = number(form, 'branch_id');
  await postManualJournal({
    postingDate: date,
    description,
    sourceType: 'manual',
    sourceId: crypto.randomUUID(),
    sourceEventKey: `manual:${crypto.randomUUID()}`,
    branchId,
    lines: [
      { accountCode: text(form, 'debit_account'), description, debitThb: amount, branchId },
      { accountCode: text(form, 'credit_account'), description, creditThb: amount, branchId },
    ],
  }, { id: actor.id, permissions: actor.permissions });
  revalidatePath('/accounting');
  revalidatePath('/ledger');
  revalidatePath('/reports');
  revalidatePath('/executive');
}

export async function reverseJournalAction(form: FormData) {
  const actor = await requireActor();
  const journalId = number(form, 'journal_id');
  await reverseJournal({
    journalId,
    postingDate: text(form, 'posting_date'),
    reason: text(form, 'reason'),
    sourceEventKey: `reversal:${journalId}:${text(form, 'request_key') || crypto.randomUUID()}`,
    actor: { id: actor.id, permissions: actor.permissions },
  });
  revalidatePath('/accounting');
  revalidatePath('/ledger');
  revalidatePath('/reports');
  revalidatePath('/executive');
}

export async function voidJournalAction(form: FormData) {
  const actor = await requireActor();
  await voidJournal(number(form, 'journal_id'), { id: actor.id, permissions: actor.permissions });
  revalidatePath('/accounting');
  revalidatePath('/ledger');
}
