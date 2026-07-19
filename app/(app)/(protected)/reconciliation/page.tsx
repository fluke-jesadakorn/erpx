import 'server-only';
import Link from 'next/link';
import { redirect } from 'next/navigation';
import { query } from '@/db';
import { PageLayout } from '@/components/PageLayout';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm/grammar';
import { BankImportPanel } from './BankImportPanel';
import { confirmBankMatchAction, createBankAccountAction, reopenBankMatchAction } from './_actions';

export const dynamic = 'force-dynamic';

const money = (value: unknown) => Number(value ?? 0).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });

export default async function ReconciliationPage() {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  if (!matchPerm(actor.permissions, 'finance:bank:import::allow') && !matchPerm(actor.permissions, 'finance:bank:match::allow')) redirect('/');
  const [accounts, branches, currencies, glAccounts, transactions, imports, groups, journals] = await Promise.all([
    query<{ id: string; code: string; bank_name: string; account_name: string; currency_code: string; gl_account_code: string }>(`SELECT id::text, code, bank_name, account_name, currency_code, gl_account_code FROM finance.bank_accounts WHERE active ORDER BY code`),
    query<{ id: string; code: string; name: string }>(`SELECT id::text, code, name FROM finance.branches WHERE active ORDER BY code`),
    query<{ code: string }>(`SELECT code FROM finance.currencies WHERE active ORDER BY code`),
    query<{ code: string; name: string }>(`SELECT code, name FROM finance.accounts WHERE active AND control_type IN ('bank','cash') ORDER BY code`),
    query<{ id: string; account_code: string; transaction_date: string; description: string; reference: string | null; amount: string; currency_code: string; status: string; suggested_journal_id: string | null; suggested_difference: string | null }>(
      `SELECT t.id::text, a.code AS account_code, t.transaction_date::text, t.description,
              t.reference, t.amount::text, t.currency_code, t.status,
              suggestion.id::text AS suggested_journal_id,
              suggestion.difference::text AS suggested_difference
         FROM finance.bank_transactions t
         JOIN finance.bank_accounts a ON a.id = t.bank_account_id
         LEFT JOIN LATERAL (
           SELECT j.id, abs(abs(t.amount) - sum(l.debit_thb)) AS difference
             FROM finance.journals j JOIN finance.journal_lines l ON l.journal_id = j.id
            WHERE j.status = 'posted' AND j.posting_date BETWEEN t.transaction_date - 7 AND t.transaction_date + 7
            GROUP BY j.id ORDER BY difference, abs(j.posting_date - t.transaction_date) LIMIT 1
         ) suggestion ON true
        WHERE t.status <> 'matched'
        ORDER BY t.transaction_date DESC, t.id DESC LIMIT 100`,
    ),
    query<{ id: string; file_name: string; row_count: number; imported_at: string; account_code: string }>(`SELECT i.id::text, i.file_name, i.row_count, i.imported_at::text, a.code AS account_code FROM finance.bank_imports i JOIN finance.bank_accounts a ON a.id = i.bank_account_id ORDER BY i.imported_at DESC LIMIT 20`),
    query<{ id: string; account_code: string; status: string; difference_thb: string; confirmed_at: string; confirmed_by_name: string | null; reopen_reason: string | null }>(`SELECT g.id::text, a.code AS account_code, g.status, g.difference_thb::text, g.confirmed_at::text, u.fullname AS confirmed_by_name, g.reopen_reason FROM finance.bank_match_groups g JOIN finance.bank_accounts a ON a.id = g.bank_account_id LEFT JOIN folio.users u ON u.id = g.confirmed_by ORDER BY g.confirmed_at DESC LIMIT 30`),
    query<{ id: string; journal_no: string; posting_date: string; description: string; total: string }>(`SELECT j.id::text, j.journal_no, j.posting_date::text, j.description, sum(l.debit_thb)::text AS total FROM finance.journals j JOIN finance.journal_lines l ON l.journal_id = j.id WHERE j.status = 'posted' GROUP BY j.id ORDER BY j.posting_date DESC, j.id DESC LIMIT 200`),
  ]);
  const canImport = matchPerm(actor.permissions, 'finance:bank:import::allow');
  const canMatch = matchPerm(actor.permissions, 'finance:bank:match::allow');
  const canReopen = matchPerm(actor.permissions, 'finance:bank:reopen::allow');
  const mappedAccounts = accounts.rows.map((account) => ({ id: Number(account.id), code: account.code, bankName: account.bank_name, currency: account.currency_code.trim() }));
  return <PageLayout title="Bank reconciliation" subtitle="Validated CSV/XLSX imports, duplicate protection, suggested matches, explicit confirmation, and audited reopening." category={{ label: 'Accounting', icon: 'ArrowDownUp', href: '/reconciliation' }} width="wide" actions={<Link className="glass-chip" href="/ledger">Ledger</Link>}>
    <div className="grid gap-5 xl:grid-cols-[minmax(0,1.4fr)_minmax(320px,0.6fr)]">
      {canImport && <BankImportPanel accounts={mappedAccounts} />}
      {canImport && <section className="panel-elevated p-5"><h2 className="text-lg font-bold">Add bank account</h2><form action={createBankAccountAction} className="mt-4 space-y-3"><label className="block text-sm">Code<input className="field" name="code" required /></label><label className="block text-sm">Bank<input className="field" name="bank_name" required /></label><label className="block text-sm">Account name<input className="field" name="account_name" required /></label><label className="block text-sm">Masked account number<input className="field" name="account_number_masked" placeholder="xxx-x-xx123-x" required /></label><label className="block text-sm">Branch<select className="field" name="branch_id">{branches.rows.map((branch) => <option key={branch.id} value={branch.id}>{branch.code} · {branch.name}</option>)}</select></label><label className="block text-sm">Currency<select className="field" name="currency_code">{currencies.rows.map((currency) => <option key={currency.code}>{currency.code}</option>)}</select></label><label className="block text-sm">GL account<select className="field" name="gl_account_code">{glAccounts.rows.map((account) => <option key={account.code} value={account.code}>{account.code} · {account.name}</option>)}</select></label><button className="action-button w-full">Create bank account</button></form></section>}
    </div>

    {canMatch && (
      <section className="panel-elevated mt-5 p-5">
        <h2 className="text-lg font-bold">Group match</h2>
        <p className="mt-1 text-sm text-ink-2">Enter comma-separated statement-row and posted-journal IDs for one-to-many or many-to-one confirmation.</p>
        <form action={confirmBankMatchAction} className="mt-4 grid gap-3 md:grid-cols-5">
          <label className="text-sm md:col-span-2">Statement row IDs<input className="field" name="bank_transaction_ids" placeholder="12, 13" required /></label>
          <label className="text-sm md:col-span-2">Journal IDs<input className="field" name="journal_ids" placeholder="40, 41" required /></label>
          <label className="text-sm">Bank fee THB<input className="field" name="fee_thb" type="number" min="0" step="0.01" defaultValue="0" /></label>
          <label className="text-sm">FX difference THB<input className="field" name="fx_difference_thb" type="number" step="0.01" defaultValue="0" /></label>
          <button className="action-button self-end md:col-span-4">Confirm group match</button>
        </form>
      </section>
    )}

    <section className="panel-elevated mt-5 overflow-hidden"><div className="border-b border-rule px-5 py-4"><h2 className="text-lg font-bold">Unmatched statement rows</h2><p className="text-sm text-ink-2">Suggestions are date-and-amount candidates only; confirmation remains explicit.</p></div><div className="overflow-x-auto"><table className="w-full text-sm"><thead className="bg-paper-2 text-left text-xs uppercase text-mute"><tr><th className="px-4 py-3">Transaction</th><th className="px-4 py-3">Description</th><th className="px-4 py-3 text-right">Amount</th><th className="px-4 py-3">Suggested journal</th><th className="px-4 py-3">Confirm</th></tr></thead><tbody className="divide-y divide-rule">{transactions.rows.map((row) => <tr key={row.id}><td className="px-4 py-3"><div>{row.transaction_date}</div><div className="font-mono text-xs text-mute">{row.account_code} · #{row.id}</div></td><td className="px-4 py-3">{row.description}<div className="text-xs text-mute">{row.reference}</div></td><td className={`px-4 py-3 text-right font-mono font-bold ${Number(row.amount) >= 0 ? 'text-positive' : 'text-critical'}`}>{money(row.amount)} {row.currency_code}</td><td className="px-4 py-3">{row.suggested_journal_id ? <Link className="text-accent" href={`/ledger/${row.suggested_journal_id}`}>Journal #{row.suggested_journal_id}</Link> : '—'}{row.suggested_difference && <div className="text-xs text-mute">Difference {money(row.suggested_difference)}</div>}</td><td className="px-4 py-3">{canMatch && <form action={confirmBankMatchAction} className="flex min-w-[280px] gap-2"><input type="hidden" name="bank_transaction_ids" value={row.id} /><select className="min-w-0 flex-1 rounded-md border border-rule bg-paper px-2 py-1 text-xs" name="journal_ids" defaultValue={row.suggested_journal_id ?? ''} required><option value="">Select posted journal</option>{journals.rows.map((journal) => <option key={journal.id} value={journal.id}>{journal.journal_no} · {money(journal.total)}</option>)}</select><input type="hidden" name="fee_thb" value="0" /><input type="hidden" name="fx_difference_thb" value={row.suggested_difference ?? '0'} /><button className="action-button">Match</button></form>}</td></tr>)}{!transactions.rows.length && <tr><td colSpan={5} className="px-4 py-8 text-center text-mute">All imported rows are matched.</td></tr>}</tbody></table></div></section>

    <div className="mt-5 grid gap-5 lg:grid-cols-2"><section className="panel-elevated overflow-hidden"><div className="border-b border-rule px-5 py-4"><h2 className="text-lg font-bold">Import history</h2></div><div className="divide-y divide-rule">{imports.rows.map((item) => <div key={item.id} className="flex items-center justify-between px-5 py-3 text-sm"><div><div className="font-medium">{item.file_name}</div><div className="text-xs text-mute">{item.account_code} · {item.imported_at}</div></div><div className="font-mono">{item.row_count} rows</div></div>)}{!imports.rows.length && <p className="p-5 text-sm text-mute">No imports yet.</p>}</div></section><section className="panel-elevated overflow-hidden"><div className="border-b border-rule px-5 py-4"><h2 className="text-lg font-bold">Match audit</h2></div><div className="divide-y divide-rule">{groups.rows.map((group) => <div key={group.id} className="p-4 text-sm"><div className="flex items-center justify-between"><div className="font-medium">Group #{group.id} · {group.account_code}</div><span className={group.status === 'confirmed' ? 'text-positive' : 'text-caution'}>{group.status}</span></div><div className="mt-1 text-xs text-mute">{group.confirmed_at} · {group.confirmed_by_name} · difference {money(group.difference_thb)}</div>{group.reopen_reason && <div className="mt-1 text-xs text-critical">{group.reopen_reason}</div>}{canReopen && group.status === 'confirmed' && <form action={reopenBankMatchAction} className="mt-3 flex gap-2"><input type="hidden" name="group_id" value={group.id} /><input className="min-w-0 flex-1 rounded-md border border-rule bg-paper px-2 py-1" name="reason" placeholder="Reason to reopen" required /><button className="glass-chip">Reopen</button></form>}</div>)}{!groups.rows.length && <p className="p-5 text-sm text-mute">No confirmed matches yet.</p>}</div></section></div>
  </PageLayout>;
}
