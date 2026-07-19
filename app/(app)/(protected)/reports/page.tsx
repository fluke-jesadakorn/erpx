import 'server-only';
import type { ReactNode } from 'react';
import { redirect } from 'next/navigation';
import Link from 'next/link';
import { ArrowRight, Building2, CalendarDays, CheckCircle2, Filter, ShieldCheck, Sparkles } from 'lucide-react';
import { PageLayout } from '@/components/PageLayout';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm/grammar';
import { loadBranches, loadExecutiveFinance } from '@/finance/reporting';
import { ExecutiveFinanceDashboard } from '@/components/finance/ExecutiveFinanceDashboard';
import { bangkokDate } from '@/date';
import { resolveReport, type ReportIntent } from '@/finance/reports';
import { FinancialReportView } from '@/components/finance/FinancialReportView';

export const dynamic = 'force-dynamic';

const intents: Record<string, ReportIntent> = {
  profit_and_loss: 'income_statement',
  gross_margin: 'gross_margin',
  balance_sheet: 'balance_sheet',
  cash_flow: 'cash_flow',
  trial_balance: 'trial_balance',
  period_summary: 'period_summary',
  ar_aging: 'ar_aging',
  ap_aging: 'ap_aging',
  fx_exposure: 'fx_exposure',
  inventory: 'inventory_valuation',
  vat_register: 'vat_register',
  wht_register: 'wht_register',
  budget_vs_actual: 'budget_vs_actual',
};

const links = [
  ['overview', 'Overview'],
  ['profit_and_loss', 'P&L'],
  ['gross_margin', 'Gross margin'],
  ['balance_sheet', 'Balance sheet'],
  ['cash_flow', 'Cash flow'],
  ['trial_balance', 'Trial balance'],
  ['ar_aging', 'AR aging'],
  ['ap_aging', 'AP aging'],
  ['fx_exposure', 'FX exposure'],
  ['inventory', 'Inventory'],
  ['vat_register', 'VAT'],
  ['wht_register', 'WHT'],
  ['budget_vs_actual', 'Budget'],
  ['period_summary', 'Journals'],
] as const;

export default async function ReportsPage({ searchParams }: { searchParams: Promise<{ from?: string; to?: string; branch?: string; report?: string }> }) {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  if (!matchPerm(actor.permissions, 'finance:report:view::allow') && !matchPerm(actor.permissions, 'finance:cashflow:read::allow')) redirect('/');
  const params = await searchParams;
  const today = bangkokDate();
  const from = params.from && /^\d{4}-\d{2}-\d{2}$/.test(params.from) ? params.from : `${today.slice(0, 5)}01-01`;
  const to = params.to && /^\d{4}-\d{2}-\d{2}$/.test(params.to) ? params.to : today;
  const branchId = Number(params.branch ?? 0) || null;
  const selected = params.report ?? 'overview';
  const intent = intents[selected];
  const [data, branches, report] = await Promise.all([
    intent ? Promise.resolve(null) : loadExecutiveFinance({ dateFrom: from, dateTo: to, branchId }),
    loadBranches(),
    intent ? resolveReport({ intent, dateFrom: from, dateTo: to, branchId, lang: 'en' }) : Promise.resolve(null),
  ]);
  const suffix = `from=${from}&to=${to}${branchId ? `&branch=${branchId}` : ''}`;
  const branch = branches.find((item) => item.id === branchId);
  const canUseAi = matchPerm(actor.permissions, 'ai:chat:use::allow');

  return <PageLayout width="wide" contentClassName="space-y-5">
    <section className="panel-elevated relative isolate overflow-hidden px-5 py-7 sm:px-8 sm:py-10">
      <div aria-hidden className="absolute -left-24 -top-36 -z-10 h-80 w-80 rounded-full bg-info/15 blur-3xl" />
      <div aria-hidden className="absolute -bottom-40 right-0 -z-10 h-96 w-96 rounded-full bg-accent/15 blur-3xl" />
      <span aria-hidden className="absolute inset-x-10 top-0 h-px bg-gradient-to-r from-transparent via-accent to-transparent" />
      <div className="flex flex-wrap items-start justify-between gap-8">
        <div className="max-w-4xl">
          <div className="flex flex-wrap items-center gap-2">
            <span className="inline-flex items-center gap-2 rounded-full border border-accent/50 bg-accent-soft px-3 py-1.5 text-[10px] font-semibold uppercase tracking-[0.16em] text-accent-ink"><Sparkles size={13} /> Executive financial intelligence</span>
            <span className="inline-flex items-center gap-1.5 rounded-full border border-positive/45 bg-positive-soft px-3 py-1.5 text-[10px] font-semibold uppercase tracking-[0.13em] text-positive"><CheckCircle2 size={12} /> Posted source of truth</span>
          </div>
          <h1 className="mt-6 max-w-3xl text-[clamp(2.25rem,5vw,4.75rem)] font-semibold leading-[0.95] tracking-[-0.065em] text-ink">Read the business.<br /><span className="text-accent">Model the next move.</span></h1>
          <p className="mt-5 max-w-2xl text-sm leading-6 text-ink-2 sm:text-base">One executive workspace for financial projection, board-level summaries, control audit, account drill-down, and scenario simulation—grounded in the live ledger.</p>
        </div>
        <div className="grid min-w-[15rem] gap-2 text-xs">
          <Scope icon={<CalendarDays size={15} />} label="Reporting period" value={`${from} → ${to}`} />
          <Scope icon={<Building2 size={15} />} label="Entity scope" value={branch ? `${branch.code} · ${branch.name}` : 'All branches'} />
          <Scope icon={<ShieldCheck size={15} />} label="AI intelligence" value={canUseAi ? 'Enabled for your role' : 'Permission required'} tone={canUseAi ? 'text-positive' : 'text-caution'} />
        </div>
      </div>
    </section>

    <div className="sticky top-2 z-20 space-y-2">
      <nav className="glass-toolbar flex gap-1 overflow-x-auto p-1.5" aria-label="Financial reports">
        {links.map(([key, label]) => <Link className={`shrink-0 rounded-xl px-3 py-2 text-xs font-medium transition ${selected === key ? 'bg-action text-action-ink shadow-sm' : 'text-ink-2 hover:bg-paper-2 hover:text-ink'}`} href={`/reports?report=${key}&${suffix}`} key={key}>{label}</Link>)}
      </nav>
    </div>

    <form className="panel-elevated grid gap-3 p-4 sm:grid-cols-[1fr_1fr_1.2fr_auto] sm:items-end">
      <input type="hidden" name="report" value={selected} />
      <label className="text-xs font-medium text-ink-2"><span className="flex items-center gap-1.5"><CalendarDays size={13} /> From</span><input className="field mt-1.5" name="from" type="date" defaultValue={from} /></label>
      <label className="text-xs font-medium text-ink-2"><span className="flex items-center gap-1.5"><CalendarDays size={13} /> To</span><input className="field mt-1.5" name="to" type="date" defaultValue={to} /></label>
      <label className="text-xs font-medium text-ink-2"><span className="flex items-center gap-1.5"><Building2 size={13} /> Branch</span><select className="field mt-1.5" name="branch" defaultValue={branchId ?? ''}><option value="">All branches</option>{branches.map((item) => <option key={item.id} value={item.id}>{item.code} · {item.name}</option>)}</select></label>
      <button className="action-button inline-flex h-10 items-center justify-center gap-2 rounded-xl border border-action/70 bg-action px-4 text-xs font-semibold text-action-ink transition hover:bg-action-hover"><Filter size={14} /> Apply scope <ArrowRight size={13} /></button>
    </form>

    {report ? <FinancialReportView report={report} canUseAi={canUseAi} /> : data ? <ExecutiveFinanceDashboard data={data} canUseAi={canUseAi} /> : null}
  </PageLayout>;
}

function Scope({ icon, label, value, tone = 'text-ink' }: { icon: ReactNode; label: string; value: string; tone?: string }) {
  return <div className="flex items-center gap-3 rounded-xl border border-rule bg-paper/55 px-3 py-3"><span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-paper-2 text-accent">{icon}</span><span><span className="block text-[10px] font-semibold uppercase tracking-[0.12em] text-mute">{label}</span><span className={`mt-0.5 block font-medium ${tone}`}>{value}</span></span></div>;
}
