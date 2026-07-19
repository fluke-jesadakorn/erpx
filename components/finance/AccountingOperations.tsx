import Link from 'next/link';
import { ArrowRight, Banknote, CircleDollarSign, FileCheck2, Landmark, ReceiptText, ShoppingCart, type LucideIcon } from 'lucide-react';
import type { AccountingOps, OpsFlow } from '@/finance/operations';

const money = (value: number, compact = false) => new Intl.NumberFormat('en-US', {
  minimumFractionDigits: compact ? 0 : 2,
  maximumFractionDigits: compact ? 1 : 2,
  notation: compact ? 'compact' : 'standard',
}).format(value);

const FLOW_ICON: Record<OpsFlow['id'], LucideIcon> = {
  capital: Landmark,
  expense: ReceiptText,
  sales: ShoppingCart,
};

function Flow({ flow }: { flow: OpsFlow }) {
  const Icon = FLOW_ICON[flow.id];
  return (
    <article className="panel-elevated overflow-hidden">
      <div className="flex flex-wrap items-start justify-between gap-3 border-b border-rule px-5 py-4">
        <div className="flex min-w-0 gap-3">
          <span className="grid size-10 shrink-0 place-items-center rounded-lg border border-rule bg-paper-2 text-accent"><Icon className="size-5" aria-hidden /></span>
          <div>
            <h3 className="font-bold text-ink">{flow.title}</h3>
            <p className="mt-0.5 max-w-xl text-sm text-ink-2">{flow.subtitle}</p>
          </div>
        </div>
        <div className="text-right">
          <div className="font-mono text-lg font-bold text-ink">{flow.openCount}</div>
          <div className="text-xs text-mute">open · THB {money(flow.openAmount, true)}</div>
        </div>
      </div>
      <div className="grid gap-0 md:grid-cols-4">
        {flow.steps.map((step, index) => (
          <div className="relative border-b border-rule px-4 py-4 last:border-b-0 md:border-b-0 md:border-r md:last:border-r-0" key={step.label}>
            <div className="mb-3 flex items-center gap-2">
              <span className="grid size-6 shrink-0 place-items-center rounded-full border border-accent/30 bg-accent-soft font-mono text-[11px] font-bold text-accent">{index + 1}</span>
              {index < flow.steps.length - 1 && <span className="hidden h-px flex-1 bg-rule md:block" aria-hidden />}
            </div>
            <div className="text-sm font-semibold text-ink">{step.label}</div>
            <div className="mt-1 text-xs text-mute">{step.owner}</div>
            <div className="mt-3 flex items-end justify-between gap-2">
              <span className="font-mono text-xl font-bold text-ink">{step.count}</span>
              <span className="text-right font-mono text-[11px] text-ink-2">THB {money(step.amount, true)}</span>
            </div>
          </div>
        ))}
      </div>
      <Link className="flex items-center justify-between border-t border-rule px-5 py-3 text-sm font-semibold text-accent transition hover:bg-paper-2" href={flow.href}>
        Open operating workflow <ArrowRight className="size-4" aria-hidden />
      </Link>
    </article>
  );
}

export function AccountingOperations({ data }: { data: AccountingOps }) {
  const metrics = [
    { label: 'Cash and bank balance', value: `THB ${money(data.cashBalance, true)}`, detail: 'Posted GL balance', icon: Banknote },
    { label: 'Posted this month', value: `THB ${money(data.postedThisMonth, true)}`, detail: `${data.postedJournalCount} journals`, icon: CircleDollarSign },
    { label: 'Posting queue', value: String(data.pendingJournalCount), detail: 'Draft or prepared', icon: FileCheck2 },
  ];
  return (
    <div className="mb-5 space-y-5">
      <section aria-labelledby="accounting-pulse-title">
        <div className="mb-3">
          <div className="text-xs font-bold uppercase tracking-[0.18em] text-accent">Operating GL</div>
          <h2 id="accounting-pulse-title" className="mt-1 text-xl font-bold text-ink">From business event to posted evidence</h2>
          <p className="mt-1 text-sm text-ink-2">Unlike the executive summary, this view shows who acts next and where each transaction enters or clears the ledger.</p>
        </div>
        <div className="grid gap-3 sm:grid-cols-3">
          {metrics.map((metric) => {
            const Icon = metric.icon;
            return <div className="panel-elevated p-4" key={metric.label}><div className="flex items-center justify-between gap-3"><span className="text-sm text-ink-2">{metric.label}</span><Icon className="size-4 text-accent" aria-hidden /></div><div className="mt-3 font-mono text-2xl font-bold text-ink">{metric.value}</div><div className="mt-1 text-xs text-mute">{metric.detail}</div></div>;
          })}
        </div>
      </section>

      <section className="space-y-4" aria-label="Finance operating workflows">
        {data.flows.map((flow) => <Flow flow={flow} key={flow.id} />)}
      </section>

      <section className="panel-elevated overflow-hidden">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-rule px-5 py-4"><div><h2 className="text-lg font-bold text-ink">Recent GL postings</h2><p className="text-sm text-ink-2">Latest immutable accounting evidence across operations.</p></div><Link className="secondary-button" href="/ledger">View full ledger</Link></div>
        <div className="divide-y divide-rule">
          {data.recent.map((row) => <Link className="flex flex-wrap items-center justify-between gap-3 px-5 py-3 transition hover:bg-paper-2" href={`/ledger/${row.id}`} key={row.id}><div className="min-w-0"><div className="font-mono text-sm font-bold text-accent">{row.journalNo}</div><div className="truncate text-sm text-ink">{row.description}</div><div className="mt-0.5 text-xs text-mute">{row.postingDate} · {row.sourceType.replaceAll('_', ' ')}</div></div><div className="font-mono font-bold text-ink">THB {money(row.amount)}</div></Link>)}
          {!data.recent.length && <div className="px-5 py-8 text-center text-sm text-mute">No posted journals yet.</div>}
        </div>
      </section>
    </div>
  );
}
