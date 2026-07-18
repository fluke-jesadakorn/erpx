'use client';

import { useEffect, useState } from 'react';
import {
  LineChart,
  Line,
  BarChart,
  Bar,
  ResponsiveContainer,
  XAxis,
  YAxis,
  Tooltip,
} from 'recharts';
import { T } from '@/components/i18n/T';

const STORAGE_KEY = 'folio.pinned_charts';

type PinnedKind = 'line' | 'bar';

interface PinnedChart {
  id: string;
  kind: PinnedKind;
  title: string;
  xKey: string;
  yKey: string;
  color?: string;
  data: Array<Record<string, number | string>>;
}

function readStored(): PinnedChart[] {
  if (typeof window === 'undefined') return [];
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (x): x is PinnedChart =>
        x && typeof x.id === 'string' &&
        (x.kind === 'line' || x.kind === 'bar') &&
        Array.isArray((x as PinnedChart).data),
    );
  } catch {
    return [];
  }
}

export function TodaysBriefPinnedCharts() {
  const [charts, setCharts] = useState<PinnedChart[]>([]);

  useEffect(() => {
    setCharts(readStored());
    const onUpdate = () => setCharts(readStored());
    window.addEventListener('folio:pinned_charts', onUpdate);
    window.addEventListener('storage', onUpdate);
    return () => {
      window.removeEventListener('folio:pinned_charts', onUpdate);
      window.removeEventListener('storage', onUpdate);
    };
  }, []);

  if (charts.length === 0) return null;

  return (
    <section className="rounded-md border border-rule bg-paper-2/50 p-4">
      <div className="flex items-center justify-between mb-3">
        <div className="text-xs font-mono font-bold uppercase text-ink-2 tracking-wider">
          📌 <T id="cockpit.pinnedCharts" />
        </div>
        <div className="text-xs font-mono text-mute">
          {charts.length}{' '}
          <T id={charts.length === 1 ? 'cockpit.pinnedChartOne' : 'cockpit.pinnedCharts'} />
        </div>
      </div>
      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
        {charts.map((chart) => (
          <div
            key={chart.id}
            className="min-w-0 rounded-md border border-rule bg-paper-2/60 p-3"
          >
            <div className="flex items-center justify-between mb-2">
              <div className="text-sm font-mono font-bold text-ink">
                {chart.title}
              </div>
              <div className="text-xs font-mono text-mute">
                {chart.kind === 'line' ? '📈' : '📊'}
              </div>
            </div>
            <div className="h-[100px] w-full">
              <ResponsiveContainer width="100%" height="100%">
                {chart.kind === 'line' ? (
                  <LineChart data={chart.data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
                    <XAxis dataKey={chart.xKey} hide />
                    <YAxis hide />
                    <Tooltip
                      contentStyle={{
                        background: '#0f172a',
                        border: '1px solid #334155',
                        borderRadius: 4,
                        fontSize: 11,
                      }}
                    />
                    <Line
                      type="monotone"
                      dataKey={chart.yKey}
                      stroke={chart.color ?? '#a78bfa'}
                      strokeWidth={2}
                      dot={{ r: 2 }}
                      isAnimationActive={false}
                    />
                  </LineChart>
                ) : (
                  <BarChart data={chart.data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
                    <XAxis dataKey={chart.xKey} hide />
                    <YAxis hide />
                    <Tooltip
                      contentStyle={{
                        background: '#0f172a',
                        border: '1px solid #334155',
                        borderRadius: 4,
                        fontSize: 11,
                      }}
                    />
                    <Bar dataKey={chart.yKey} fill={chart.color ?? '#a78bfa'} />
                  </BarChart>
                )}
              </ResponsiveContainer>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

export default TodaysBriefPinnedCharts;