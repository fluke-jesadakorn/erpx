import 'server-only';
import { redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm/grammar';
import { PageLayout } from '@/components/PageLayout';
import { loadBranches, loadExecutiveFinance } from '@/finance/reporting';
import { ExecutiveFinanceDashboard } from '@/components/finance/ExecutiveFinanceDashboard';
import { bangkokDate } from '@/date';

export const dynamic = 'force-dynamic';

export default async function ExecutivePage({ searchParams }: { searchParams: Promise<{ from?: string; to?: string; branch?: string }> }) {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  if (!matchPerm(actor.permissions, 'tile:executive:view::allow') && !matchPerm(actor.permissions, 'finance:report:executive::allow')) redirect('/');
  const params = await searchParams;
  const today = bangkokDate();
  const from = params.from && /^\d{4}-\d{2}-\d{2}$/.test(params.from) ? params.from : `${today.slice(0, 5)}01-01`;
  const to = params.to && /^\d{4}-\d{2}-\d{2}$/.test(params.to) ? params.to : today;
  const branchId = Number(params.branch ?? 0) || null;
  const [data, branches] = await Promise.all([loadExecutiveFinance({ dateFrom: from, dateTo: to, branchId }), loadBranches()]);
  return <PageLayout title="Executive finance" subtitle={`${actor.fullname} · Posted actuals, operational pipeline, forecasts, and exact drill-downs`} category={{ label: 'Executive', icon: 'Star', href: '/executive' }} width="wide"><form className="panel-elevated mb-5 grid gap-3 p-4 sm:grid-cols-[1fr_1fr_1fr_auto]"><label className="text-sm">From<input className="field" name="from" type="date" defaultValue={from} /></label><label className="text-sm">To<input className="field" name="to" type="date" defaultValue={to} /></label><label className="text-sm">Branch<select className="field" name="branch" defaultValue={branchId ?? ''}><option value="">All branches</option>{branches.map((branch) => <option key={branch.id} value={branch.id}>{branch.code} · {branch.name}</option>)}</select></label><button className="action-button self-end">Apply filters</button></form><ExecutiveFinanceDashboard data={data} /><div className="mt-4 text-right text-xs text-mute">Live posted data generated {new Date(data.generatedAt).toLocaleString('en-GB')}</div></PageLayout>;
}
