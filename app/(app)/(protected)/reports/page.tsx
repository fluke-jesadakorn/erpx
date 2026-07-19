import 'server-only';
import { redirect } from 'next/navigation';
import Link from 'next/link';
import { PageLayout } from '@/components/PageLayout';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm/grammar';
import { loadBranches, loadExecutiveFinance } from '@/finance/reporting';
import { ExecutiveFinanceDashboard } from '@/components/finance/ExecutiveFinanceDashboard';
import { bangkokDate } from '@/date';
import { resolveReport, type ReportIntent } from '@/finance/reports';
import { FinancialReportView } from '@/components/finance/FinancialReportView';

export const dynamic = 'force-dynamic';

const reportIntents: Record<string, ReportIntent> = {
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

const reportLinks = [
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
  ['budget_vs_actual', 'Budget vs actual'],
  ['period_summary', 'Journal summary'],
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
  const intent = reportIntents[selected];
  const [data, branches, report] = await Promise.all([
    intent ? Promise.resolve(null) : loadExecutiveFinance({ dateFrom: from, dateTo: to, branchId }),
    loadBranches(),
    intent ? resolveReport({ intent, dateFrom: from, dateTo: to, branchId, lang: 'en' }) : Promise.resolve(null),
  ]);
  const suffix = `from=${from}&to=${to}${branchId ? `&branch=${branchId}` : ''}`;
  return <PageLayout title="Financial reports" subtitle="Posted-only financial statements, subledgers, tax registers, inventory valuation, budgets, FX exposure, and control tie-outs." category={{ label: 'Accounting', icon: 'Gauge', href: '/reports' }} width="wide">
    <nav className="mb-5 flex flex-wrap gap-2">{reportLinks.map(([key, label]) => <Link className={selected === key ? 'action-button' : 'glass-chip'} href={`/reports?report=${key}&${suffix}`} key={key}>{label}</Link>)}</nav>
    <form className="panel-elevated mb-5 grid gap-3 p-4 sm:grid-cols-[1fr_1fr_1fr_auto]"><input type="hidden" name="report" value={selected} /><label className="text-sm">From<input className="field" name="from" type="date" defaultValue={from} /></label><label className="text-sm">To<input className="field" name="to" type="date" defaultValue={to} /></label><label className="text-sm">Branch<select className="field" name="branch" defaultValue={branchId ?? ''}><option value="">All branches</option>{branches.map((branch) => <option key={branch.id} value={branch.id}>{branch.code} · {branch.name}</option>)}</select></label><button className="action-button self-end">Apply filters</button></form>
    {report ? <FinancialReportView report={report} /> : data ? <ExecutiveFinanceDashboard data={data} /> : null}
  </PageLayout>;
}
