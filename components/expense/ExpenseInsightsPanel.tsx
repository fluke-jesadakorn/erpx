import type { ExpenseInsights } from '@/finance/expenseInsights';
import { Icon } from '@/components/icons';
import { Kpi, Panel } from '@/components/ui';

const thb = (n: number) => n.toLocaleString('th-TH', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
const pct = (p: number, t: number) => (t ? `${Math.round((p / t) * 100)}%` : '0%');

type DeltaTone = 'positive' | 'caution' | 'critical' | 'neutral';

function toneFor(value: 'positive' | 'negative' | 'warning' | 'neutral'): DeltaTone {
  if (value === 'positive') return 'positive';
  if (value === 'negative') return 'critical';
  if (value === 'warning') return 'caution';
  return 'neutral';
}

export function ExpenseInsightsPanel({ insights, scope }: { insights: ExpenseInsights; scope: string }) {
  const headline =
    insights.count === 0
      ? 'No expense records yet'
      : `${insights.count} records · ${thb(insights.total)} THB total · avg ${thb(insights.avg)} THB`;

  return (
    <Panel tone="elevated" className="space-y-4">
      <header className="flex flex-wrap items-baseline justify-between gap-3">
        <div className="flex items-center gap-2 text-xs font-medium uppercase tracking-wider text-mute">
          <Icon name="bar-chart" size={14} className="text-accent" />
          <span>Expense Insights · {scope}</span>
        </div>
        <span className="text-xs text-mute">{headline}</span>
      </header>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <Kpi title="Total" value={`${thb(insights.total)} ฿`} delta={{ value: `${insights.count} records`, tone: toneFor('neutral') }} />
        <Kpi title="Disbursed" value={`${thb(insights.disbursed)} ฿`} delta={{ value: pct(insights.disbursed, insights.total), tone: toneFor('positive') }} />
        <Kpi title="In pipeline" value={`${thb(insights.pending)} ฿`} delta={{ value: pct(insights.pending, insights.total), tone: toneFor('warning') }} />
        <Kpi title="Rejected" value={`${thb(insights.rejected)} ฿`} delta={{ value: pct(insights.rejected, insights.total), tone: toneFor('negative') }} />
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