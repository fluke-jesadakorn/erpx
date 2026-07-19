import 'server-only';
import { redirect } from 'next/navigation';
import { query } from '@/db';
import { PageLayout } from '@/components/PageLayout';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm/grammar';
import { approveBudgetAction, createBudgetAction, saveBudgetLineAction } from './_actions';

export const dynamic = 'force-dynamic';

const money = (value: unknown) => Number(value ?? 0).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });

export default async function BudgetsPage() {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  if (!matchPerm(actor.permissions, 'finance:budget:view::allow') && !matchPerm(actor.permissions, 'finance:budget:manage::allow')) redirect('/');
  const [budgets, branches, accounts, lines] = await Promise.all([
    query<{ id: string; name: string; fiscal_year: number; branch_name: string | null; status: string; total: string; actual: string }>(
      `SELECT b.id::text, b.name, b.fiscal_year, br.name AS branch_name, b.status,
              coalesce(sum(bl.amount_thb), 0)::text AS total,
              coalesce((SELECT sum(CASE WHEN a.account_type = 'expense' THEN l.debit_thb - l.credit_thb ELSE 0 END)
                          FROM finance.v_posted_lines l JOIN finance.accounts a ON a.code = l.account_code
                         WHERE extract(year from l.posting_date) = b.fiscal_year
                           AND (b.branch_id IS NULL OR l.branch_id = b.branch_id)), 0)::text AS actual
         FROM finance.budgets b
         LEFT JOIN finance.branches br ON br.id = b.branch_id
         LEFT JOIN finance.budget_lines bl ON bl.budget_id = b.id
        GROUP BY b.id, br.name ORDER BY b.fiscal_year DESC, b.name`,
    ),
    query<{ id: string; code: string; name: string }>(`SELECT id::text, code, name FROM finance.branches WHERE active ORDER BY code`),
    query<{ code: string; name: string }>(`SELECT code, name FROM finance.accounts WHERE active AND account_type IN ('revenue','expense') ORDER BY code`),
    query<{ budget_id: string; period_no: number; account_code: string; account_name: string; department_id: string | null; amount_thb: string }>(`SELECT bl.budget_id::text, bl.period_no, bl.account_code, a.name AS account_name, bl.department_id, bl.amount_thb::text FROM finance.budget_lines bl JOIN finance.accounts a ON a.code = bl.account_code ORDER BY bl.budget_id, bl.period_no, bl.account_code`),
  ]);
  const canManage = matchPerm(actor.permissions, 'finance:budget:manage::allow');
  const year = new Date().getFullYear();
  return <PageLayout title="Budgets" subtitle="Approved monthly account budgets compared with posted actuals. Drafts remain outside executive variance reporting." category={{ label: 'Finance', icon: 'Gauge', href: '/budgets' }} width="wide"><div className="grid gap-5 xl:grid-cols-[minmax(0,1.4fr)_minmax(320px,0.6fr)]"><section className="space-y-4">{budgets.rows.map((budget) => { const budgetLines = lines.rows.filter((line) => line.budget_id === budget.id); return <article key={budget.id} className="panel-elevated overflow-hidden"><div className="flex flex-wrap items-center justify-between gap-3 border-b border-rule px-5 py-4"><div><h2 className="font-bold">{budget.name} · {budget.fiscal_year}</h2><p className="text-sm text-mute">{budget.branch_name ?? 'All branches'} · {budget.status}</p></div><div className="text-right"><div className="font-mono font-bold">THB {money(budget.total)}</div><div className={`text-xs ${Number(budget.total) - Number(budget.actual) >= 0 ? 'text-positive' : 'text-critical'}`}>Variance {money(Number(budget.total) - Number(budget.actual))}</div></div>{canManage && budget.status === 'draft' && <form action={approveBudgetAction}><input type="hidden" name="budget_id" value={budget.id} /><button className="action-button">Approve budget</button></form>}</div><div className="max-h-72 overflow-auto"><table className="w-full text-sm"><thead className="bg-paper-2 text-left text-xs uppercase text-mute"><tr><th className="px-4 py-2">Period</th><th className="px-4 py-2">Account</th><th className="px-4 py-2">Department</th><th className="px-4 py-2 text-right">Amount</th></tr></thead><tbody>{budgetLines.map((line, index) => <tr key={`${line.period_no}-${line.account_code}-${line.department_id ?? index}`} className="border-t border-rule"><td className="px-4 py-2">{line.period_no}</td><td className="px-4 py-2">{line.account_code} · {line.account_name}</td><td className="px-4 py-2">{line.department_id ?? 'All'}</td><td className="px-4 py-2 text-right font-mono">{money(line.amount_thb)}</td></tr>)}{!budgetLines.length && <tr><td colSpan={4} className="p-5 text-center text-mute">No budget lines.</td></tr>}</tbody></table></div></article>; })}{!budgets.rows.length && <div className="panel-elevated p-8 text-center text-mute">No budgets yet.</div>}</section>{canManage && <div className="space-y-5"><section className="panel-elevated p-5"><h2 className="text-lg font-bold">Create budget</h2><form action={createBudgetAction} className="mt-4 space-y-3"><label className="block text-sm">Name<input className="field" name="name" required /></label><label className="block text-sm">Fiscal year<input className="field" type="number" name="fiscal_year" defaultValue={year} required /></label><label className="block text-sm">Branch<select className="field" name="branch_id"><option value="">All branches</option>{branches.rows.map((branch) => <option key={branch.id} value={branch.id}>{branch.code} · {branch.name}</option>)}</select></label><button className="action-button w-full">Create draft</button></form></section><section className="panel-elevated p-5"><h2 className="text-lg font-bold">Add budget line</h2><form action={saveBudgetLineAction} className="mt-4 space-y-3"><label className="block text-sm">Draft budget<select className="field" name="budget_id">{budgets.rows.filter((budget) => budget.status === 'draft').map((budget) => <option key={budget.id} value={budget.id}>{budget.name} · {budget.fiscal_year}</option>)}</select></label><label className="block text-sm">Month<select className="field" name="period_no">{Array.from({ length: 12 }, (_, index) => <option key={index + 1} value={index + 1}>{index + 1}</option>)}</select></label><label className="block text-sm">Account<select className="field" name="account_code">{accounts.rows.map((account) => <option key={account.code} value={account.code}>{account.code} · {account.name}</option>)}</select></label><label className="block text-sm">Department<input className="field" name="department_id" placeholder="Optional" /></label><label className="block text-sm">Amount THB<input className="field" type="number" step="0.01" name="amount_thb" required /></label><button className="action-button w-full">Save line</button></form></section></div>}</div></PageLayout>;
}
