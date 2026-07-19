import 'server-only';
import { redirect } from 'next/navigation';
import Link from 'next/link';
import { query } from '@/db';
import { PageLayout } from '@/components/PageLayout';
import { AccountingOperations } from '@/components/finance/AccountingOperations';
import { ManualJournalComposer } from '@/components/finance/ManualJournalComposer';
import { Button } from '@/components/ui/Button';
import { loadAccountingOps } from '@/finance/operations';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm/grammar';
import { postPreparedAction, reverseJournalAction, voidJournalAction } from './_actions';

export const dynamic = 'force-dynamic';

const money = (value: unknown) => Number(value ?? 0).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });

export default async function AccountingPage() {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  const canView = matchPerm(actor.permissions, 'finance:ledger:view::allow') || matchPerm(actor.permissions, 'finance:journal:prepare::allow');
  if (!canView) redirect('/');
  const [queue, accounts, branches, periods, operations] = await Promise.all([
    query<{
      id: number;
      journal_no: string | null;
      status: string;
      posting_date: string;
      description: string;
      source_type: string;
      source_id: string;
      preparer_id: number | null;
      preparer: string | null;
      debit: string;
      credit: string;
    }>(
      `SELECT j.id, j.journal_no, j.status, j.posting_date::text, j.description,
              j.source_type, j.source_id, j.preparer_id, u.fullname AS preparer,
              sum(l.debit_thb)::text AS debit, sum(l.credit_thb)::text AS credit
         FROM finance.journals j
         JOIN finance.journal_lines l ON l.journal_id = j.id
         LEFT JOIN folio.users u ON u.id = j.preparer_id
        WHERE j.status IN ('draft','prepared')
        GROUP BY j.id, u.fullname
        ORDER BY CASE j.status WHEN 'prepared' THEN 1 ELSE 2 END, j.posting_date, j.id`,
    ),
    query<{ code: string; name: string; account_type: string }>(`SELECT code, name, account_type FROM finance.accounts WHERE active ORDER BY code`),
    query<{ id: string; code: string; name: string }>(`SELECT id::text, code, name FROM finance.branches WHERE active ORDER BY code`),
    query<{ id: string; fiscal_year: number; period_no: number; starts_on: string; ends_on: string; status: string }>(
      `SELECT id::text, fiscal_year, period_no, starts_on::text, ends_on::text, status
         FROM finance.fiscal_periods
        WHERE ends_on >= current_date - interval '2 months'
        ORDER BY starts_on LIMIT 15`,
    ),
    loadAccountingOps(),
  ]);
  const bypass = matchPerm(actor.permissions, 'admin:system:bypass::allow');
  const canApprove = matchPerm(actor.permissions, 'finance:journal:approve::allow') || bypass;
  const canPrepare = matchPerm(actor.permissions, 'finance:journal:prepare::allow') || bypass;
  const canManual = (matchPerm(actor.permissions, 'finance:journal:manual::allow') || bypass) && canPrepare;
  const canReverse = matchPerm(actor.permissions, 'finance:journal:reverse::allow') || bypass;
  const canVerifyCapital = canApprove && (actor.department === 'finance' || actor.department === 'accounting' || bypass);
  const today = new Date().toISOString().slice(0, 10);

  return (
    <PageLayout
      title="Accounting workspace"
      subtitle="Prepare, approve, post, void, and reverse controlled journals. Only posted journals reach financial reports."
      category={{ label: 'Finance', icon: 'BookOpen', href: '/accounting' }}
      width="wide"
      actions={<Link className="action-button inline-flex h-10 items-center justify-center rounded-md border border-action/70 bg-action px-3.5 text-sm font-medium text-action-ink transition hover:-translate-y-px hover:bg-action-hover" href="/ledger">Open ledger</Link>}
    >
      <AccountingOperations data={operations} />

      {canManual && (
        <ManualJournalComposer
          accounts={accounts.rows.map((account) => ({
            code: account.code,
            name: account.name,
            accountType: account.account_type,
          }))}
          branches={branches.rows}
          canPost={canApprove}
          initialDate={today}
          initialRequestKey={crypto.randomUUID()}
        />
      )}

      <div className={canReverse ? 'grid gap-5 xl:grid-cols-[minmax(0,1.5fr)_minmax(320px,0.8fr)]' : ''}>
        <section className="panel-elevated overflow-hidden">
          <div className="border-b border-rule px-5 py-4">
            <h2 className="text-lg font-bold text-ink">Preparation and approval queue</h2>
            <p className="text-sm text-ink-2">{queue.rows.length} unposted journal{queue.rows.length === 1 ? '' : 's'}</p>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="bg-paper-2 text-left text-xs uppercase tracking-wider text-mute">
                <tr><th className="px-4 py-3">Journal</th><th className="px-4 py-3">Source</th><th className="px-4 py-3 text-right">Debit / credit</th><th className="px-4 py-3">Action</th></tr>
              </thead>
              <tbody className="divide-y divide-rule">
                {queue.rows.map((row) => (
                  <tr key={row.id}>
                    <td className="px-4 py-3 align-top">
                      <Link className="font-mono font-bold text-accent" href={`/ledger/${row.id}`}>{row.journal_no ?? `Draft #${row.id}`}</Link>
                      <div className="mt-1 text-ink">{row.description}</div>
                      <div className="text-xs text-mute">{row.posting_date} · {row.status} · {row.preparer ?? 'not prepared'}</div>
                    </td>
                    <td className="px-4 py-3 align-top font-mono text-xs text-ink-2">{row.source_type}<br />{row.source_id}</td>
                    <td className="px-4 py-3 align-top text-right font-mono tabular-nums">{money(row.debit)}<br /><span className={Number(row.debit) === Number(row.credit) ? 'text-positive' : 'text-critical'}>{money(row.credit)}</span></td>
                    <td className="px-4 py-3 align-top">
                      <div className="flex flex-wrap gap-2">
                        {row.status === 'prepared' && canApprove && (row.source_type !== 'capital_contribution' || (canVerifyCapital && row.preparer_id !== actor.id)) && (
                          <form action={postPreparedAction}><input type="hidden" name="journal_id" value={row.id} /><Button size="sm" type="submit" variant="primary">Approve & post</Button></form>
                        )}
                        {row.status !== 'posted' && canPrepare && (
                          <form action={voidJournalAction}><input type="hidden" name="journal_id" value={row.id} /><Button size="sm" type="submit" variant="secondary">Void</Button></form>
                        )}
                      </div>
                    </td>
                  </tr>
                ))}
                {!queue.rows.length && <tr><td className="px-4 py-8 text-center text-mute" colSpan={4}>The queue is clear.</td></tr>}
              </tbody>
            </table>
          </div>
        </section>

        {canReverse && (
          <div className="space-y-5">
            <section className="panel-elevated p-5">
              <h2 className="text-lg font-bold text-ink">Reverse posted journal</h2>
              <form action={reverseJournalAction} className="mt-4 space-y-3">
                <label className="block text-sm text-ink-2">Journal ID<input className="field" name="journal_id" type="number" min="1" required /></label>
                <label className="block text-sm text-ink-2">Reversal date<input className="field" name="posting_date" type="date" defaultValue={today} required /></label>
                <label className="block text-sm text-ink-2">Reason<input className="field" name="reason" required /></label>
                <input type="hidden" name="request_key" value={crypto.randomUUID()} />
                <Button className="w-full" type="submit" variant="secondary">Create linked reversal</Button>
              </form>
            </section>
          </div>
        )}
      </div>

      <section className="panel-elevated mt-5 p-5">
        <div className="flex flex-wrap items-center justify-between gap-3"><div><h2 className="text-lg font-bold text-ink">Fiscal periods</h2><p className="text-sm text-ink-2">Posting is accepted only in an open period.</p></div><Link className="glass-input inline-flex h-10 items-center justify-center rounded-md px-3.5 text-sm font-medium text-ink transition hover:-translate-y-px hover:bg-paper-3/70" href="/accounting/periods">Manage close checklist</Link></div>
        <div className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {periods.rows.map((period) => <div key={period.id} className="rounded-md border border-rule bg-paper-2 p-3"><div className="font-mono text-xs text-mute">{period.fiscal_year} / {String(period.period_no).padStart(2, '0')}</div><div className="mt-1 text-sm text-ink">{period.starts_on} → {period.ends_on}</div><div className={`mt-2 text-xs font-bold uppercase ${period.status === 'open' ? 'text-positive' : period.status === 'locked' ? 'text-critical' : 'text-caution'}`}>{period.status.replace('_', ' ')}</div></div>)}
        </div>
      </section>
    </PageLayout>
  );
}
