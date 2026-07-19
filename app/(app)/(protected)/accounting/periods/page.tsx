import 'server-only';
import Link from 'next/link';
import { redirect } from 'next/navigation';
import { query } from '@/db';
import { PageLayout } from '@/components/PageLayout';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm/grammar';
import { setPeriodStatusAction, updateClosingTaskAction } from './_actions';

export const dynamic = 'force-dynamic';

export default async function PeriodsPage() {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  if (!matchPerm(actor.permissions, 'finance:period:close::allow') && !matchPerm(actor.permissions, 'finance:period:reopen::allow')) redirect('/accounting');
  const [periods, tasks] = await Promise.all([
    query<{ id: string; fiscal_year: number; period_no: number; starts_on: string; ends_on: string; status: string }>(`SELECT id::text, fiscal_year, period_no, starts_on::text, ends_on::text, status FROM finance.fiscal_periods WHERE ends_on >= current_date - interval '12 months' ORDER BY starts_on`),
    query<{ id: string; period_id: string; task_key: string; label: string; status: string; completed_by_name: string | null }>(`SELECT c.id::text, c.period_id::text, c.task_key, c.label, c.status, u.fullname AS completed_by_name FROM finance.closing_checklists c LEFT JOIN folio.users u ON u.id = c.completed_by JOIN finance.fiscal_periods p ON p.id = c.period_id WHERE p.ends_on >= current_date - interval '12 months' ORDER BY p.starts_on, c.id`),
  ]);
  const canClose = matchPerm(actor.permissions, 'finance:period:close::allow');
  const canReopen = matchPerm(actor.permissions, 'finance:period:reopen::allow');
  return <PageLayout title="Period close" subtitle="Soft close, hard lock, controlled reopen, and evidence checklist for each monthly fiscal period." category={{ label: 'Accounting', icon: 'Calendar', href: '/accounting' }} width="wide" actions={<Link className="glass-chip" href="/accounting">Back to accounting</Link>}><div className="space-y-5">{periods.rows.map((period) => { const list = tasks.rows.filter((task) => task.period_id === period.id); const done = list.filter((task) => task.status !== 'pending').length; return <section key={period.id} className="panel-elevated overflow-hidden"><div className="flex flex-wrap items-center justify-between gap-3 border-b border-rule px-5 py-4"><div><h2 className="font-bold">{period.fiscal_year} / {String(period.period_no).padStart(2, '0')} · {period.starts_on} → {period.ends_on}</h2><p className="text-sm text-mute">{done}/{list.length} closing controls complete · <span className={period.status === 'open' ? 'text-positive' : period.status === 'locked' ? 'text-critical' : 'text-caution'}>{period.status}</span></p></div><div className="flex gap-2">{canClose && period.status === 'open' && <form action={setPeriodStatusAction}><input type="hidden" name="period_id" value={period.id} /><input type="hidden" name="status" value="soft_closed" /><button className="glass-chip">Soft close</button></form>}{canClose && period.status !== 'locked' && <form action={setPeriodStatusAction}><input type="hidden" name="period_id" value={period.id} /><input type="hidden" name="status" value="locked" /><button className="action-button">Hard lock</button></form>}{canReopen && period.status !== 'open' && <form action={setPeriodStatusAction}><input type="hidden" name="period_id" value={period.id} /><input type="hidden" name="status" value="open" /><button className="glass-chip">Reopen</button></form>}</div></div><div className="grid gap-3 p-5 md:grid-cols-2">{list.map((task) => <div key={task.id} className="flex items-center justify-between gap-3 rounded-md border border-rule bg-paper-2 p-3"><div><div className="text-sm font-medium">{task.label}</div><div className="text-xs text-mute">{task.completed_by_name ? `${task.status} by ${task.completed_by_name}` : task.status}</div></div>{canClose && <form action={updateClosingTaskAction}><input type="hidden" name="task_id" value={task.id} /><select className="rounded-md border border-rule bg-paper px-2 py-1 text-sm" name="status" defaultValue={task.status}><option value="pending">Pending</option><option value="done">Done</option><option value="waived">Waived</option></select><button className="ml-2 text-sm text-accent">Save</button></form>}</div>)}</div></section>; })}</div></PageLayout>;
}
