import 'server-only';
import Link from 'next/link';
import { redirect } from 'next/navigation';
import { ArrowRight, Banknote, CalendarDays, CheckCircle2, Hash, Landmark, Scale, StickyNote } from 'lucide-react';
import { PageLayout } from '@/components/PageLayout';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm/grammar';
import { canCreateCapital, canVerifyCapital, loadCapitalWorkspace } from '@/finance/capital';
import { submitCapitalContributionAction, verifyCapitalContributionAction } from './_actions';
import { Button } from '@/components/ui/Button';
import { FormField } from '@/components/ui/FormField';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';

export const dynamic = 'force-dynamic';

const money = (value: number) => value.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
const tone = (status: string) => status === 'posted' ? 'text-positive' : status === 'void' ? 'text-critical' : status === 'prepared' ? 'text-caution' : 'text-mute';

export default async function CapitalPage() {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  const allowed = matchPerm(actor.permissions, 'admin:system:bypass::allow')
    || matchPerm(actor.permissions, 'tile:capital:view::allow')
    || matchPerm(actor.permissions, 'finance:journal:prepare::allow')
    || matchPerm(actor.permissions, 'finance:journal:approve::allow');
  if (!allowed) redirect('/');
  const data = await loadCapitalWorkspace();
  const capitalActor = {
    id: actor.id,
    fullname: actor.fullname,
    roleName: actor.role_name,
    departmentId: actor.dept_id,
    permissions: actor.permissions,
  };
  const canCreate = canCreateCapital(capitalActor);
  const canVerify = canVerifyCapital(capitalActor);
  const pending = data.contributions.filter((row) => row.status === 'prepared');
  const posted = data.contributions.filter((row) => row.status === 'posted');
  const today = new Date().toISOString().slice(0, 10);

  return (
    <PageLayout
      title="Capital contributions"
      subtitle="CEO funding, independent Finance verification, and a balanced posted GL entry."
      category={{ label: 'Finance', icon: 'BookOpen', href: '/capital' }}
      width="wide"
      actions={(
        <div className="flex gap-2">
          <Link className="inline-flex h-10 items-center justify-center rounded-md border border-rule bg-paper-2 px-3.5 text-sm font-medium text-ink-2 transition-colors hover:bg-paper-3 hover:text-ink" href="/accounting">Accounting operations</Link>
          <Link className="inline-flex h-10 items-center justify-center rounded-md border border-action/70 bg-action px-3.5 text-sm font-medium text-action-ink transition-colors hover:bg-action-hover" href="/ledger">Open ledger</Link>
        </div>
      )}
    >
      <section className="panel-elevated overflow-hidden">
        <div className="border-b border-rule px-5 py-4">
          <div className="text-xs font-bold uppercase tracking-[0.18em] text-accent">Controlled workflow</div>
          <h2 className="mt-1 text-xl font-bold text-ink">Cash enters the business only after two-person control</h2>
        </div>
        <div className="grid md:grid-cols-3">
          {[
            { icon: Landmark, title: '1 · CEO records funding', text: 'Choose cash or bank, branch, equity account, amount, and source reference.' },
            { icon: Scale, title: '2 · Finance verifies', text: 'A different Finance or Accounting approver checks the source, amount, and balanced proposal.' },
            { icon: CheckCircle2, title: '3 · GL posts', text: 'Debit bank/cash and credit equity. Reports change only after posting.' },
          ].map((step) => {
            const IconCmp = step.icon;
            return (
              <div className="border-b border-rule p-5 last:border-b-0 md:border-b-0 md:border-r md:last:border-r-0" key={step.title}>
                <IconCmp className="size-5 text-accent" aria-hidden />
                <h3 className="mt-3 font-bold text-ink">{step.title}</h3>
                <p className="mt-1 text-sm text-ink-2">{step.text}</p>
              </div>
            );
          })}
        </div>
      </section>

      <div className="mt-5 grid gap-5 xl:grid-cols-[minmax(320px,0.65fr)_minmax(0,1.35fr)]">
        <div className="space-y-5">
          <section className="grid grid-cols-2 gap-3">
            <div className="panel-elevated p-4">
              <div className="text-sm text-ink-2">Awaiting verification</div>
              <div className="mt-2 font-mono text-2xl font-bold text-caution">{pending.length}</div>
              <div className="mt-1 text-xs text-mute">THB {money(pending.reduce((sum, row) => sum + row.amount, 0))}</div>
            </div>
            <div className="panel-elevated p-4">
              <div className="text-sm text-ink-2">Posted capital</div>
              <div className="mt-2 font-mono text-2xl font-bold text-positive">THB {money(posted.reduce((sum, row) => sum + row.amount, 0))}</div>
              <div className="mt-1 text-xs text-mute">{posted.length} verified entries</div>
            </div>
          </section>

          {canCreate ? (
            <section className="panel-elevated overflow-hidden">
              <header className="flex items-start gap-3 border-b border-rule/70 bg-accent-soft/30 px-5 py-4">
                <span className="grid size-9 shrink-0 place-items-center rounded-xl border border-accent/35 bg-accent-soft/70 text-accent">
                  <Banknote size={17} aria-hidden />
                </span>
                <div className="min-w-0">
                  <h2 className="text-base font-bold tracking-tight text-ink">Record company funding</h2>
                  <p className="mt-0.5 text-xs text-ink-2">Prepares a journal for independent verification. The GL does not change yet.</p>
                </div>
              </header>

              <form action={submitCapitalContributionAction} className="space-y-4 p-5">
                <input type="hidden" name="request_key" value={crypto.randomUUID()} />

                <div className="grid gap-4 sm:grid-cols-2">
                  <FormField label="Contribution date" required>
                    <Input name="posting_date" type="date" defaultValue={today} required leftIcon={<CalendarDays size={14} />} />
                  </FormField>
                  <FormField label="Branch" required>
                    <Select name="branch_id" required>
                      {data.branches.map((branch) => (
                        <option key={branch.id} value={branch.id}>{branch.code} · {branch.name}</option>
                      ))}
                    </Select>
                  </FormField>
                </div>

                <FormField label="Deposit to" required hint="Cash or bank account that receives the funds.">
                  <Select name="funding_account_code" required>
                    {data.fundingAccounts.map((account) => (
                      <option key={account.code} value={account.code}>
                        {account.controlType === 'bank' ? 'Bank transfer' : 'Cash'} · {account.code} · {account.name}
                      </option>
                    ))}
                  </Select>
                </FormField>

                <FormField label="Credit equity" required hint="Equity account that balances the deposit.">
                  <Select name="equity_account_code" required>
                    {data.equityAccounts.map((account) => (
                      <option key={account.code} value={account.code}>{account.code} · {account.name}</option>
                    ))}
                  </Select>
                </FormField>

                <FormField label="Amount (THB)" required>
                  <Input name="amount" type="number" min="0.01" step="0.01" required inputMode="decimal" placeholder="0.00" leftIcon={<span className="text-xs font-mono font-bold text-mute">THB</span>} />
                </FormField>

                <FormField label="Bank reference" hint="Required for bank transfer; optional for cash.">
                  <Input name="reference" placeholder="e.g. TRANSFER-2024-001" leftIcon={<Hash size={14} />} />
                </FormField>

                <FormField label="Note">
                  <Input name="note" placeholder="Purpose or supporting detail" leftIcon={<StickyNote size={14} />} />
                </FormField>

                <div className="flex flex-col gap-2 border-t border-rule/60 pt-4 sm:flex-row sm:items-center sm:justify-between">
                  <p className="text-xs text-mute">A different verifier must approve before posting.</p>
                  <Button type="submit" variant="primary" size="lg" rightIcon={<ArrowRight size={15} />}>Send to Finance verification</Button>
                </div>
              </form>
            </section>
          ) : (
            <section className="panel-elevated p-5">
              <h2 className="font-bold text-ink">Independent control</h2>
              <p className="mt-1 text-sm text-ink-2">Only the CEO records company funding. Your role can {canVerify ? 'verify submitted contributions' : 'review posted evidence'}.</p>
            </section>
          )}
        </div>

        <section className="panel-elevated overflow-hidden">
          <div className="border-b border-rule px-5 py-4">
            <h2 className="text-lg font-bold text-ink">Contribution register</h2>
            <p className="text-sm text-ink-2">Prepared items wait for a different verifier; posted items link to immutable evidence.</p>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="bg-paper-2 text-left text-xs uppercase tracking-wider text-mute">
                <tr>
                  <th className="px-4 py-3">Contribution</th>
                  <th className="px-4 py-3">Funding</th>
                  <th className="px-4 py-3 text-right">Amount</th>
                  <th className="px-4 py-3">Control</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-rule">
                {data.contributions.map((row) => (
                  <tr key={row.id} className="transition-colors hover:bg-paper-2/40">
                    <td className="px-4 py-3 align-top">
                      <Link className="font-mono font-bold text-accent hover:underline" href={`/ledger/${row.id}`}>{row.journalNo ?? `Prepared #${row.id}`}</Link>
                      <div className="mt-1 text-ink">{row.postingDate} · {row.preparerName ?? 'Unknown contributor'}</div>
                      <div className={`mt-1 text-xs font-bold uppercase ${tone(row.status)}`}>{row.status}</div>
                    </td>
                    <td className="px-4 py-3 align-top">
                      <div className="text-ink">{row.fundingMethod === 'cash' ? 'Cash' : 'Bank transfer'} · {row.fundingAccountCode}</div>
                      <div className="text-xs text-mute">{row.fundingAccountName} → {row.equityAccountCode}</div>
                      {row.reference && <div className="mt-1 font-mono text-xs text-ink-2">Ref {row.reference}</div>}
                    </td>
                    <td className="px-4 py-3 text-right align-top font-mono font-bold text-ink">THB {money(row.amount)}</td>
                    <td className="px-4 py-3 align-top">
                      {row.status === 'prepared' && canVerify && row.preparerId !== actor.id ? (
                        <form action={verifyCapitalContributionAction}>
                          <input type="hidden" name="journal_id" value={row.id} />
                          <Button type="submit" variant="positive" size="sm">Verify &amp; post</Button>
                        </form>
                      ) : row.status === 'posted' ? (
                        <div className="text-xs text-positive">Verified by {row.approverName ?? 'Finance'}
                          <Link className="mt-1 flex items-center gap-1 text-accent hover:underline" href={`/ledger/${row.id}`}>View GL <ArrowRight className="size-3" aria-hidden /></Link>
                        </div>
                      ) : (
                        <span className="text-xs text-mute">{row.preparerId === actor.id ? 'Waiting for another verifier' : 'No action available'}</span>
                      )}
                    </td>
                  </tr>
                ))}
                {!data.contributions.length && (
                  <tr><td className="px-4 py-10 text-center text-mute" colSpan={4}>No capital contributions recorded yet.</td></tr>
                )}
              </tbody>
            </table>
          </div>
        </section>
      </div>
    </PageLayout>
  );
}
