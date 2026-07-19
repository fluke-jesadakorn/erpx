import 'server-only';
import Link from 'next/link';
import { redirect } from 'next/navigation';
import { query } from '@/db';
import { PageLayout } from '@/components/PageLayout';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm/grammar';
import { saveAccountAction, toggleAccountAction } from './_actions';

export const dynamic = 'force-dynamic';

interface Props {
  searchParams: Promise<{ tab?: string; q?: string; account?: string; from?: string; to?: string }>;
}

const money = (value: unknown) => Number(value ?? 0).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });

export default async function LedgerPage({ searchParams }: Props) {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  if (!matchPerm(actor.permissions, 'finance:ledger:view::allow') && !matchPerm(actor.permissions, 'finance:gl:view::allow')) redirect('/');
  const params = await searchParams;
  const tab = params.tab === 'accounts' ? 'accounts' : 'journals';
  const q = String(params.q ?? '').trim();
  const account = String(params.account ?? '').trim() || null;
  const to = params.to && /^\d{4}-\d{2}-\d{2}$/.test(params.to) ? params.to : new Date().toISOString().slice(0, 10);
  const from = params.from && /^\d{4}-\d{2}-\d{2}$/.test(params.from) ? params.from : `${to.slice(0, 8)}01`;
  const [journals, accounts, trial] = await Promise.all([
    query<{
      id: number;
      journal_no: string | null;
      status: string;
      posting_date: string;
      description: string;
      source_type: string;
      source_id: string;
      debit: string;
      credit: string;
    }>(
      `SELECT j.id, j.journal_no, j.status, j.posting_date::text, j.description,
              j.source_type, j.source_id, sum(l.debit_thb)::text AS debit, sum(l.credit_thb)::text AS credit
         FROM finance.journals j
         JOIN finance.journal_lines l ON l.journal_id = j.id
        WHERE j.posting_date BETWEEN $1::date AND $2::date
          AND ($3::text = '' OR j.description ILIKE '%' || $3 || '%' OR j.journal_no ILIKE '%' || $3 || '%' OR j.source_id ILIKE '%' || $3 || '%')
          AND ($4::text IS NULL OR EXISTS (SELECT 1 FROM finance.journal_lines x WHERE x.journal_id = j.id AND x.account_code = $4))
        GROUP BY j.id
        ORDER BY j.posting_date DESC, j.id DESC LIMIT 250`,
      [from, to, q, account],
    ),
    query<{ code: string; name: string; name_th: string | null; account_type: string; normal_side: string; control_type: string | null; active: boolean }>(
      `SELECT code, name, name_th, account_type, normal_side, control_type, active
         FROM finance.accounts
        WHERE $1::text = '' OR code ILIKE '%' || $1 || '%' OR name ILIKE '%' || $1 || '%' OR name_th ILIKE '%' || $1 || '%'
        ORDER BY code`,
      [q],
    ),
    query<{ code: string; name: string; account_type: string; debit: string; credit: string; balance: string }>(
      `SELECT code, name, account_type, debit::text, credit::text, balance::text
         FROM finance.trial_balance($1::date, $2::date, NULL)`,
      [from, to],
    ),
  ]);
  const canManage = matchPerm(actor.permissions, 'finance:coa:manage::allow');

  return (
    <PageLayout
      title="Ledger"
      subtitle="Posted and unposted journals, general-ledger drill-down, and the controlled chart of accounts."
      category={{ label: 'Accounting', icon: 'BookOpen', href: '/ledger' }}
      width="wide"
      actions={<div className="flex gap-2"><Link className={tab === 'journals' ? 'action-button' : 'glass-chip'} href="/ledger">Journals</Link><Link className={tab === 'accounts' ? 'action-button' : 'glass-chip'} href="/ledger?tab=accounts">Chart of accounts</Link></div>}
    >
      <form className="panel-elevated mb-5 grid gap-3 p-4 sm:grid-cols-[1fr_auto_auto_auto]">
        <input type="hidden" name="tab" value={tab} />
        <input className="rounded-md border border-rule bg-paper px-3 py-2 text-sm" name="q" defaultValue={q} placeholder="Search journal, source, account…" />
        <input className="rounded-md border border-rule bg-paper px-3 py-2 text-sm" name="from" type="date" defaultValue={from} />
        <input className="rounded-md border border-rule bg-paper px-3 py-2 text-sm" name="to" type="date" defaultValue={to} />
        <button className="action-button" type="submit">Apply</button>
      </form>

      {tab === 'journals' ? (
        <div className="space-y-5">
          <section className="panel-elevated overflow-hidden">
            <div className="overflow-x-auto"><table className="w-full text-sm"><thead className="bg-paper-2 text-left text-xs uppercase tracking-wider text-mute"><tr><th className="px-4 py-3">Date / journal</th><th className="px-4 py-3">Description</th><th className="px-4 py-3">Source</th><th className="px-4 py-3 text-right">Debit</th><th className="px-4 py-3 text-right">Credit</th><th className="px-4 py-3">Status</th></tr></thead><tbody className="divide-y divide-rule">{journals.rows.map((row) => <tr key={row.id}><td className="px-4 py-3"><div className="text-xs text-mute">{row.posting_date}</div><Link className="font-mono font-bold text-accent" href={`/ledger/${row.id}`}>{row.journal_no ?? `#${row.id}`}</Link></td><td className="px-4 py-3 text-ink">{row.description}</td><td className="px-4 py-3 font-mono text-xs text-ink-2">{row.source_type}<br />{row.source_id}</td><td className="px-4 py-3 text-right font-mono">{money(row.debit)}</td><td className="px-4 py-3 text-right font-mono">{money(row.credit)}</td><td className="px-4 py-3"><span className={`glass-chip ${row.status === 'posted' ? 'text-positive' : row.status === 'void' ? 'text-critical' : 'text-caution'}`}>{row.status}</span></td></tr>)}{!journals.rows.length && <tr><td colSpan={6} className="px-4 py-8 text-center text-mute">No journals in this period.</td></tr>}</tbody></table></div>
          </section>
          <section className="panel-elevated overflow-hidden"><div className="border-b border-rule px-5 py-4"><h2 className="text-lg font-bold text-ink">Trial balance · posted only</h2></div><div className="max-h-[480px] overflow-auto"><table className="w-full text-sm"><thead className="sticky top-0 bg-paper-2 text-left text-xs uppercase text-mute"><tr><th className="px-4 py-3">Account</th><th className="px-4 py-3 text-right">Debit</th><th className="px-4 py-3 text-right">Credit</th><th className="px-4 py-3 text-right">Balance</th></tr></thead><tbody>{trial.rows.map((row) => <tr key={row.code} className="border-t border-rule"><td className="px-4 py-3"><Link className="font-mono text-accent" href={`/ledger?account=${row.code}&from=${from}&to=${to}`}>{row.code}</Link> · {row.name}</td><td className="px-4 py-3 text-right font-mono">{money(row.debit)}</td><td className="px-4 py-3 text-right font-mono">{money(row.credit)}</td><td className="px-4 py-3 text-right font-mono font-bold">{money(row.balance)}</td></tr>)}</tbody></table></div></section>
        </div>
      ) : (
        <div className="grid gap-5 xl:grid-cols-[minmax(0,1.4fr)_minmax(320px,0.6fr)]">
          <section className="panel-elevated overflow-hidden"><div className="overflow-x-auto"><table className="w-full text-sm"><thead className="bg-paper-2 text-left text-xs uppercase text-mute"><tr><th className="px-4 py-3">Code</th><th className="px-4 py-3">Name</th><th className="px-4 py-3">Type</th><th className="px-4 py-3">Control</th><th className="px-4 py-3">State</th></tr></thead><tbody className="divide-y divide-rule">{accounts.rows.map((row) => <tr key={row.code}><td className="px-4 py-3 font-mono font-bold">{row.code}</td><td className="px-4 py-3">{row.name}<div className="text-xs text-mute">{row.name_th}</div></td><td className="px-4 py-3">{row.account_type} / {row.normal_side}</td><td className="px-4 py-3">{row.control_type ?? '—'}</td><td className="px-4 py-3">{canManage ? <form action={toggleAccountAction}><input type="hidden" name="code" value={row.code} /><button className={row.active ? 'text-positive' : 'text-critical'}>{row.active ? 'active' : 'inactive'}</button></form> : row.active ? 'active' : 'inactive'}</td></tr>)}</tbody></table></div></section>
          {canManage && <section className="panel-elevated p-5"><h2 className="text-lg font-bold">Add or update account</h2><form action={saveAccountAction} className="mt-4 space-y-3"><label className="block text-sm">Code<input className="mt-1 w-full rounded-md border border-rule bg-paper px-3 py-2" name="code" required /></label><label className="block text-sm">English name<input className="mt-1 w-full rounded-md border border-rule bg-paper px-3 py-2" name="name" required /></label><label className="block text-sm">Thai name<input className="mt-1 w-full rounded-md border border-rule bg-paper px-3 py-2" name="name_th" /></label><label className="block text-sm">Type<select className="mt-1 w-full rounded-md border border-rule bg-paper px-3 py-2" name="account_type">{['asset','liability','equity','revenue','expense'].map((type) => <option key={type}>{type}</option>)}</select></label><label className="block text-sm">Control<select className="mt-1 w-full rounded-md border border-rule bg-paper px-3 py-2" name="control_type"><option value="">None</option>{['bank','cash','ar','ap','inventory','tax','grni'].map((type) => <option key={type}>{type}</option>)}</select></label><button className="action-button w-full" type="submit">Save account</button></form></section>}
        </div>
      )}
    </PageLayout>
  );
}
