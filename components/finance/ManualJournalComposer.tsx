'use client';

import Link from 'next/link';
import { useActionState, useState } from 'react';
import { useFormStatus } from 'react-dom';
import { ArrowRight, CheckCircle2, Plus, Search, Send, Trash2 } from 'lucide-react';
import { createManualJournalAction, type ManualJournalActionState } from '@/app/(app)/(protected)/accounting/_actions';
import { Alert } from '@/components/ui/Alert';
import { Button } from '@/components/ui/Button';
import { FormField } from '@/components/ui/FormField';
import { Input } from '@/components/ui/Input';
import { Label } from '@/components/ui/Label';
import { Select } from '@/components/ui/Select';
import { Textarea } from '@/components/ui/Textarea';

interface AccountOption {
  code: string;
  name: string;
  accountType: string;
}

interface BranchOption {
  id: string;
  code: string;
  name: string;
}

interface JournalRow {
  id: string;
  accountCode: string;
  memo: string;
  debit: string;
  credit: string;
}

interface ManualJournalComposerProps {
  accounts: AccountOption[];
  branches: BranchOption[];
  canPost: boolean;
  initialDate: string;
  initialRequestKey: string;
}

const INITIAL_STATE: ManualJournalActionState = { status: 'idle', message: '' };

const initialRows = (): JournalRow[] => [
  { id: 'debit-line', accountCode: '', memo: '', debit: '', credit: '' },
  { id: 'credit-line', accountCode: '', memo: '', debit: '', credit: '' },
];

const money = (value: number) => new Intl.NumberFormat('en-US', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
}).format(value);

const title = (value: string) => value.replaceAll('_', ' ').replace(/\b\w/g, (letter) => letter.toUpperCase());

function amount(value: string) {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? Math.round(parsed * 100) : 0;
}

function SubmitButtons({ canPost, valid }: { canPost: boolean; valid: boolean }) {
  const { pending } = useFormStatus();
  return (
    <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
      <Button
        className="sm:min-w-44"
        disabled={!valid}
        leftIcon={<Send className="size-4" aria-hidden />}
        loading={pending}
        name="mode"
        type="submit"
        value="prepare"
        variant={canPost ? 'secondary' : 'primary'}
      >
        Prepare for approval
      </Button>
      {canPost && (
        <Button
          className="sm:min-w-40"
          disabled={!valid}
          leftIcon={<CheckCircle2 className="size-4" aria-hidden />}
          loading={pending}
          name="mode"
          type="submit"
          value="post"
          variant="primary"
        >
          Post now
        </Button>
      )}
    </div>
  );
}

export function ManualJournalComposer({
  accounts,
  branches,
  canPost,
  initialDate,
  initialRequestKey,
}: ManualJournalComposerProps) {
  const [state, action] = useActionState(createManualJournalAction, INITIAL_STATE);
  const [rows, setRows] = useState<JournalRow[]>(initialRows);
  const [postingDate, setPostingDate] = useState(initialDate);
  const [branchId, setBranchId] = useState(branches[0]?.id ?? '');
  const [description, setDescription] = useState('');
  const [reference, setReference] = useState('');
  const [filter, setFilter] = useState('');
  const [requestKey, setRequestKey] = useState(initialRequestKey);
  const [dismissedSubmission, setDismissedSubmission] = useState<string | undefined>();

  const query = filter.trim().toLowerCase();
  const filteredAccounts = query
    ? accounts.filter((account) => `${account.code} ${account.name} ${account.accountType}`.toLowerCase().includes(query))
    : accounts;
  const debitCents = rows.reduce((total, row) => total + amount(row.debit), 0);
  const creditCents = rows.reduce((total, row) => total + amount(row.credit), 0);
  const completeLines = rows.every((row) => {
    const debit = amount(row.debit);
    const credit = amount(row.credit);
    return Boolean(row.accountCode) && ((debit > 0) !== (credit > 0));
  });
  const distinctAccounts = new Set(rows.map((row) => row.accountCode).filter(Boolean)).size >= 2;
  const balanced = debitCents > 0 && debitCents === creditCents;
  const valid = Boolean(
    accounts.length
    && branches.length
    && postingDate
    && branchId
    && description.trim().length >= 3
    && completeLines
    && distinctAccounts
    && balanced,
  );

  let issue = '';
  if (!accounts.length) issue = 'No active general ledger accounts are configured.';
  else if (!branches.length) issue = 'No active branch is configured.';
  else if (description.trim().length < 3) issue = 'Add a clear journal description.';
  else if (!completeLines) issue = 'Complete every line with one account and either a debit or credit amount.';
  else if (!distinctAccounts) issue = 'Use at least two different accounts.';
  else if (!balanced) issue = 'Debit and credit totals must match.';

  const updateRow = (id: string, field: keyof Omit<JournalRow, 'id'>, value: string) => {
    setRows((current) => current.map((row) => {
      if (row.id !== id) return row;
      if (field === 'debit' && value) return { ...row, debit: value, credit: '' };
      if (field === 'credit' && value) return { ...row, debit: '', credit: value };
      return { ...row, [field]: value };
    }));
  };

  const addRow = () => {
    setRows((current) => [...current, {
      id: crypto.randomUUID(),
      accountCode: '',
      memo: '',
      debit: '',
      credit: '',
    }]);
  };

  const removeRow = (id: string) => {
    setRows((current) => current.length > 2 ? current.filter((row) => row.id !== id) : current);
  };

  const startNew = () => {
    setRows(initialRows());
    setDescription('');
    setReference('');
    setFilter('');
    setRequestKey(crypto.randomUUID());
    setDismissedSubmission(state.submissionId);
  };

  const showSuccess = state.status === 'success' && state.submissionId !== dismissedSubmission;

  return (
    <section className="panel-elevated mb-5 overflow-hidden" aria-labelledby="manual-journal-title">
      <div className="flex flex-wrap items-start justify-between gap-4 border-b border-rule px-5 py-4 sm:px-6">
        <div>
          <div className="text-xs font-bold uppercase tracking-[0.18em] text-accent">General ledger entry</div>
          <h2 className="mt-1 text-xl font-bold text-ink" id="manual-journal-title">Manual journal</h2>
          <p className="mt-1 max-w-3xl text-sm text-ink-2">
            Record corrections, accruals, reclassifications, and transfers with a balanced multi-line entry.
          </p>
        </div>
        <span className="glass-chip px-3 py-1.5 text-xs font-semibold text-ink-2">
          {canPost ? 'Prepare or post' : 'Approval required'}
        </span>
      </div>

      <form action={action} className="space-y-5 p-5 sm:p-6">
        {state.status === 'error' && (
          <Alert tone="critical" title="Journal was not submitted">{state.message}</Alert>
        )}
        {showSuccess && (
          <Alert
            actions={(
              <>
                {state.journalId && (
                  <Link className="glass-input inline-flex h-9 items-center justify-center gap-1.5 rounded-md px-3 text-xs font-medium text-ink transition hover:-translate-y-px hover:bg-paper-3/70" href={`/ledger/${state.journalId}`}>
                    Open journal <ArrowRight className="size-4" aria-hidden />
                  </Link>
                )}
                <Button onClick={startNew} size="sm" variant="ghost">New journal</Button>
              </>
            )}
            tone="positive"
            title="Journal submitted"
          >
            {state.message}
          </Alert>
        )}

        <div className="grid gap-4 lg:grid-cols-12">
          <FormField className="lg:col-span-3" htmlFor="journal-posting-date" label="Posting date" required>
            <Input
              id="journal-posting-date"
              name="posting_date"
              onChange={(event) => setPostingDate(event.target.value)}
              required
              type="date"
              value={postingDate}
            />
          </FormField>
          <FormField className="lg:col-span-3" htmlFor="journal-branch" label="Branch" required>
            <Select id="journal-branch" name="branch_id" onChange={(event) => setBranchId(event.target.value)} required value={branchId}>
              {!branches.length && <option value="">No active branches</option>}
              {branches.map((branch) => <option key={branch.id} value={branch.id}>{branch.code} · {branch.name}</option>)}
            </Select>
          </FormField>
          <FormField className="lg:col-span-3" htmlFor="journal-reference" hint="Optional document or internal reference" label="Reference">
            <Input
              id="journal-reference"
              maxLength={120}
              name="reference"
              onChange={(event) => setReference(event.target.value)}
              placeholder="e.g. ADJ-2026-0719"
              value={reference}
            />
          </FormField>
          <FormField className="lg:col-span-3" htmlFor="journal-account-filter" hint={`${filteredAccounts.length} of ${accounts.length} accounts shown`} label="Filter accounts">
            <Input
              id="journal-account-filter"
              leftIcon={<Search className="size-4" aria-hidden />}
              onChange={(event) => setFilter(event.target.value)}
              placeholder="Code, name, or type"
              type="search"
              value={filter}
            />
          </FormField>
          <FormField className="lg:col-span-12" htmlFor="journal-description" label="Description" required>
            <Textarea
              className="min-h-20"
              id="journal-description"
              maxLength={500}
              name="description"
              onChange={(event) => setDescription(event.target.value)}
              placeholder="Explain the business reason and supporting evidence for this entry."
              required
              value={description}
            />
          </FormField>
        </div>

        <div className="overflow-hidden rounded-lg border border-rule">
          <div className="hidden grid-cols-[minmax(220px,1.15fr)_minmax(180px,1fr)_minmax(130px,0.45fr)_minmax(130px,0.45fr)_40px] gap-3 border-b border-rule bg-paper-2 px-4 py-2.5 text-xs font-bold uppercase tracking-wider text-mute lg:grid">
            <div>Account</div><div>Line memo</div><div className="text-right">Debit (THB)</div><div className="text-right">Credit (THB)</div><span className="sr-only">Actions</span>
          </div>
          <div className="divide-y divide-rule">
            {rows.map((row, index) => {
              const selected = accounts.find((account) => account.code === row.accountCode);
              const visibleAccounts = selected && !filteredAccounts.some((account) => account.code === selected.code)
                ? [selected, ...filteredAccounts]
                : filteredAccounts;
              return (
                <div className="grid gap-3 bg-paper/45 p-4 lg:grid-cols-[minmax(220px,1.15fr)_minmax(180px,1fr)_minmax(130px,0.45fr)_minmax(130px,0.45fr)_40px] lg:items-end" key={row.id}>
                  <div>
                    <Label className="lg:sr-only" htmlFor={`journal-account-${row.id}`}>Account on line {index + 1}</Label>
                    <Select
                      id={`journal-account-${row.id}`}
                      onChange={(event) => updateRow(row.id, 'accountCode', event.target.value)}
                      required
                      value={row.accountCode}
                    >
                      <option value="">Choose account…</option>
                      {visibleAccounts.map((account) => (
                        <option key={account.code} value={account.code}>
                          {account.code} · {account.name} · {title(account.accountType)}
                        </option>
                      ))}
                    </Select>
                  </div>
                  <div>
                    <Label className="lg:sr-only" htmlFor={`journal-memo-${row.id}`}>Memo on line {index + 1}</Label>
                    <Input
                      id={`journal-memo-${row.id}`}
                      maxLength={500}
                      onChange={(event) => updateRow(row.id, 'memo', event.target.value)}
                      placeholder="Uses main description if blank"
                      value={row.memo}
                    />
                  </div>
                  <div>
                    <Label className="lg:sr-only" htmlFor={`journal-debit-${row.id}`}>Debit on line {index + 1}</Label>
                    <Input
                      className="text-right font-mono tabular-nums"
                      id={`journal-debit-${row.id}`}
                      inputMode="decimal"
                      min="0"
                      onChange={(event) => updateRow(row.id, 'debit', event.target.value)}
                      placeholder="0.00"
                      step="0.01"
                      type="number"
                      value={row.debit}
                    />
                  </div>
                  <div>
                    <Label className="lg:sr-only" htmlFor={`journal-credit-${row.id}`}>Credit on line {index + 1}</Label>
                    <Input
                      className="text-right font-mono tabular-nums"
                      id={`journal-credit-${row.id}`}
                      inputMode="decimal"
                      min="0"
                      onChange={(event) => updateRow(row.id, 'credit', event.target.value)}
                      placeholder="0.00"
                      step="0.01"
                      type="number"
                      value={row.credit}
                    />
                  </div>
                  <Button
                    aria-label={`Remove line ${index + 1}`}
                    className="justify-self-end lg:justify-self-auto"
                    disabled={rows.length <= 2}
                    onClick={() => removeRow(row.id)}
                    size="md"
                    variant="ghost"
                  >
                    <Trash2 className="size-4" aria-hidden />
                  </Button>
                </div>
              );
            })}
          </div>
          <div className="border-t border-rule bg-paper-2/70 px-4 py-3">
            <Button leftIcon={<Plus className="size-4" aria-hidden />} onClick={addRow} size="sm" variant="ghost">Add line</Button>
          </div>
        </div>

        <div className="grid gap-4 rounded-lg border border-rule bg-paper-2/55 p-4 md:grid-cols-[minmax(0,1fr)_repeat(3,minmax(130px,0.3fr))] md:items-center">
          <div>
            <div className={`text-sm font-semibold ${valid ? 'text-positive' : 'text-ink'}`}>
              {valid ? 'Balanced and ready to submit' : issue}
            </div>
            <p className="mt-1 text-xs text-mute">
              {canPost ? 'Post now writes immediately to the GL. Prepare sends the entry to the approval queue.' : 'This entry will go to the approval queue before it reaches the GL.'}
            </p>
          </div>
          <div className="md:text-right">
            <div className="text-xs uppercase tracking-wider text-mute">Debit total</div>
            <div className="mt-1 font-mono text-lg font-bold tabular-nums text-ink">{money(debitCents / 100)}</div>
          </div>
          <div className="md:text-right">
            <div className="text-xs uppercase tracking-wider text-mute">Credit total</div>
            <div className="mt-1 font-mono text-lg font-bold tabular-nums text-ink">{money(creditCents / 100)}</div>
          </div>
          <div className="md:text-right" aria-live="polite">
            <div className="text-xs uppercase tracking-wider text-mute">Difference</div>
            <div className={`mt-1 font-mono text-lg font-bold tabular-nums ${balanced ? 'text-positive' : 'text-critical'}`}>
              {money(Math.abs(debitCents - creditCents) / 100)}
            </div>
          </div>
        </div>

        <input name="lines" type="hidden" value={JSON.stringify(rows.map((row) => ({
          accountCode: row.accountCode,
          memo: row.memo,
          debit: row.debit,
          credit: row.credit,
        })))} />
        <input name="request_key" type="hidden" value={requestKey} />
        <SubmitButtons canPost={canPost} valid={valid} />
      </form>
    </section>
  );
}
