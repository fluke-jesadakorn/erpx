'use server';

import { revalidatePath } from 'next/cache';
import { requireActor } from '@/server/guard';
import { approveAndPostJournal, postManualJournal, preparePosting, reverseJournal, voidJournal } from '@/finance';
import { query } from '@/db';
import { verifyCapitalContribution } from '@/finance/capital';
import { matchPerm } from '@/perm/grammar';

const text = (form: FormData, key: string) => String(form.get(key) ?? '').trim();
const number = (form: FormData, key: string) => Number(text(form, key));

export interface ManualJournalActionState {
  status: 'idle' | 'error' | 'success';
  message: string;
  journalId?: number;
  journalNo?: string | null;
  submissionId?: string;
}

interface ManualLineInput {
  accountCode?: unknown;
  memo?: unknown;
  debit?: unknown;
  credit?: unknown;
}

function validDate(value: string) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const date = new Date(`${value}T00:00:00Z`);
  return !Number.isNaN(date.valueOf()) && date.toISOString().slice(0, 10) === value;
}

function amount(value: unknown, line: number, side: 'debit' | 'credit') {
  if (value === '' || value === null || value === undefined) return 0;
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 0) throw new Error(`Line ${line} has an invalid ${side} amount.`);
  if (parsed > 9_999_999_999_999.99) throw new Error(`Line ${line} exceeds the supported amount.`);
  return Math.round(parsed * 100) / 100;
}

function manualLines(form: FormData, description: string, branchId: number) {
  let input: unknown;
  try {
    input = JSON.parse(text(form, 'lines'));
  } catch {
    throw new Error('Journal lines could not be read. Please review them and try again.');
  }
  if (!Array.isArray(input) || input.length < 2) throw new Error('Add at least two journal lines.');
  if (input.length > 100) throw new Error('A manual journal is limited to 100 lines.');

  let debitCents = 0;
  let creditCents = 0;
  const lines = input.map((value, index) => {
    if (!value || typeof value !== 'object') throw new Error(`Line ${index + 1} is invalid.`);
    const row = value as ManualLineInput;
    const accountCode = String(row.accountCode ?? '').trim();
    const memo = String(row.memo ?? '').trim();
    const debitThb = amount(row.debit, index + 1, 'debit');
    const creditThb = amount(row.credit, index + 1, 'credit');
    if (!accountCode) throw new Error(`Choose an account on line ${index + 1}.`);
    if ((debitThb > 0) === (creditThb > 0)) throw new Error(`Line ${index + 1} must have either a debit or a credit amount.`);
    if (memo.length > 500) throw new Error(`Line ${index + 1} memo is too long.`);
    debitCents += Math.round(debitThb * 100);
    creditCents += Math.round(creditThb * 100);
    return {
      accountCode,
      description: memo || description,
      debitThb,
      creditThb,
      branchId,
    };
  });
  if (debitCents <= 0 || debitCents !== creditCents) {
    throw new Error(`Journal is not balanced: THB ${(debitCents / 100).toFixed(2)} debit / THB ${(creditCents / 100).toFixed(2)} credit.`);
  }
  if (new Set(lines.map((line) => line.accountCode)).size < 2) {
    throw new Error('Use at least two different accounts for a meaningful journal entry.');
  }
  return lines;
}

export async function postPreparedAction(form: FormData) {
  const actor = await requireActor();
  const journalId = number(form, 'journal_id');
  const source = await query<{ source_type: string }>(`SELECT source_type FROM finance.journals WHERE id = $1`, [journalId]);
  if (source.rows[0]?.source_type === 'capital_contribution') {
    await verifyCapitalContribution(journalId, {
      id: actor.id,
      fullname: actor.fullname,
      roleName: actor.role_name,
      departmentId: actor.dept_id,
      permissions: actor.permissions,
    });
  } else {
    await approveAndPostJournal(journalId, { id: actor.id, permissions: actor.permissions });
  }
  revalidatePath('/accounting');
  revalidatePath('/capital');
  revalidatePath('/ledger');
  revalidatePath('/executive');
}

export async function createManualJournalAction(
  _previous: ManualJournalActionState,
  form: FormData,
): Promise<ManualJournalActionState> {
  const actor = await requireActor();
  try {
    const bypass = matchPerm(actor.permissions, 'admin:system:bypass::allow');
    if (!matchPerm(actor.permissions, 'finance:journal:manual::allow') && !bypass) {
      throw new Error('You do not have permission to create manual journals.');
    }
    const postingDate = text(form, 'posting_date');
    const description = text(form, 'description');
    const reference = text(form, 'reference');
    const requestKey = text(form, 'request_key');
    const mode = text(form, 'mode');
    const branchId = number(form, 'branch_id');
    if (!validDate(postingDate)) throw new Error('Choose a valid posting date.');
    if (description.length < 3 || description.length > 500) throw new Error('Enter a description between 3 and 500 characters.');
    if (reference.length > 120) throw new Error('Reference must be 120 characters or fewer.');
    if (!/^[a-zA-Z0-9:_-]{8,128}$/.test(requestKey)) throw new Error('The request key is invalid. Refresh the page and try again.');
    if (mode !== 'prepare' && mode !== 'post') throw new Error('Choose whether to prepare or post the journal.');
    if (!Number.isInteger(branchId) || branchId <= 0) throw new Error('Choose an active branch.');
    const lines = manualLines(form, description, branchId);
    const accountCodes = [...new Set(lines.map((line) => line.accountCode))];
    const [branch, period, accounts, existing] = await Promise.all([
      query<{ id: string }>(`SELECT id::text FROM finance.branches WHERE id = $1 AND active`, [branchId]),
      query<{ status: string }>(
        `SELECT status FROM finance.fiscal_periods
          WHERE $1::date BETWEEN starts_on AND ends_on
          ORDER BY starts_on DESC LIMIT 1`,
        [postingDate],
      ),
      query<{ code: string }>(`SELECT code FROM finance.accounts WHERE active AND code = ANY($1::text[])`, [accountCodes]),
      query<{ id: number; journal_no: string | null; status: string }>(
        `SELECT id, journal_no, status FROM finance.journals WHERE source_event_key = $1`,
        [`manual:${requestKey}`],
      ),
    ]);
    if (!branch.rows[0]) throw new Error('Choose an active branch.');
    if (!period.rows[0]) throw new Error('The posting date is outside the configured fiscal periods.');
    if (period.rows[0].status !== 'open') throw new Error(`The fiscal period for ${postingDate} is ${period.rows[0].status.replaceAll('_', ' ')}.`);
    const activeCodes = new Set(accounts.rows.map((row) => row.code));
    const missingCode = accountCodes.find((code) => !activeCodes.has(code));
    if (missingCode) throw new Error(`Account ${missingCode} is not active or does not exist.`);

    const prior = existing.rows[0];
    if (prior) {
      if (mode === 'post' && prior.status !== 'posted') throw new Error(`Journal ${prior.journal_no ?? `#${prior.id}`} already exists with status ${prior.status}.`);
      return {
        status: 'success',
        message: `Journal ${prior.journal_no ?? `#${prior.id}`} is already ${prior.status}.`,
        journalId: prior.id,
        journalNo: prior.journal_no,
        submissionId: requestKey,
      };
    }

    const draft = {
      postingDate,
      description,
      sourceType: 'manual',
      sourceId: reference || `MJ-${requestKey.slice(0, 8).toUpperCase()}`,
      sourceEventKey: `manual:${requestKey}`,
      branchId,
      metadata: { reference: reference || null, entryMode: mode },
      lines,
    };
    const journal = mode === 'post'
      ? await postManualJournal(draft, { id: actor.id, permissions: actor.permissions })
      : await preparePosting(draft, { id: actor.id, permissions: actor.permissions });
    revalidatePath('/accounting');
    revalidatePath('/ledger');
    revalidatePath('/reports');
    revalidatePath('/executive');
    return {
      status: 'success',
      message: mode === 'post'
        ? `Journal ${journal.journalNo ?? `#${journal.id}`} was posted to the general ledger.`
        : `Journal #${journal.id} was prepared and sent to the approval queue.`,
      journalId: journal.id,
      journalNo: journal.journalNo,
      submissionId: requestKey,
    };
  } catch (error) {
    return {
      status: 'error',
      message: error instanceof Error ? error.message : 'The journal could not be submitted.',
    };
  }
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
