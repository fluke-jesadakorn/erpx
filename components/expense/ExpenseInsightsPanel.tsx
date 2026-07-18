import type { ExpenseInsights } from '@/finance/expenseInsights';
import { ChartBar } from 'lucide-react';
import { Kpi, Panel } from '@/components/ui';

const thb = (n: number) => n.toLocaleString('th-TH', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
const pct = (p: number, t: number) => (t ? `${Math.round((p / t) * 100)}%` : '0%');

export function ExpenseInsightsPanel({ insights, scope }: { insights: ExpenseInsights; scope: string }) {
  const headline =
    insights.count === 0
      ? 'No expense records yet'
      : `${insights.count} records · ${thb(insights.total)} THB total · avg ${thb(insights.avg)} THB`;

  return (
    <Panel tone="elevated" className="space-y-4">
      <header className="flex flex-wrap items-baseline justify-between gap-3">
        <div className="flex items-center gap-2 text-xs font-medium uppercase tracking-wider text-mute">
          <ChartBar size={14} className="text-accent" />
          <span>Expense Insights · {scope}</span>
        </div>
        <span className="text-xs text-mute">{headline}</span>
      </header>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Kpi label="Total" value={`${thb(insights.total)} ฿`} caption={`${insights.count} records`} />
        <Kpi label="Disbursed" value={`${thb(insights.disbursed)} ฿`} caption={pct(insights.disbursed, insights.total)} />
        <Kpi label="In pipeline" value={`${thb(insights.pending)} ฿`} caption={pct(insights.pending, insights.total)} />
        <Kpi label="Rejected" value={`${thb(insights.rejected)} ฿`} caption={pct(insights.rejected, insights.total)} />
      </div>
      {insights.byStatus.length > 0 && (
        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
          {insights.byStatus.map((s) => (
            <Panel key={s.status} padding="sm">
              <div className="flex items-center justify-between gap-2 text-xs">
                <span className="font-mono text-mute">{s.status.replace(/_/g, ' ')}</span>
                <span className="font-mono tabular-nums text-ink-2">
                  {thb(s.sum)} ฿ <span className="text-mute">· {s.count}</span>
                </span>
              </div>
            </Panel>
          ))}
        </div>
      )}
    </Panel>
  );
}