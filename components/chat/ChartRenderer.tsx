'use client';
import { useEffect, useState } from 'react';
import {
  ResponsiveContainer,
  LineChart,
  Line,
  BarChart,
  Bar,
  PieChart,
  Pie,
  Cell,
  XAxis,
  YAxis,
  Tooltip,
  Legend,
  CartesianGrid,
  AreaChart,
  Area,
} from 'recharts';
import type { ChartSpec } from './chartContract';
import { T } from '@/components/i18n/T';

const COLORS = ['#818cf8', '#f472b6', '#34d399', '#fbbf24', '#22d3ee', '#a78bfa'];

export function ChartRenderer({ spec }: { spec: ChartSpec }) {
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);

  if (!mounted) {
    return (
      <div className="text-xs text-mute font-mono py-2">
        <T id="chat.chart.rendering" />
      </div>
    );
  }

  const xLabels = spec.axes?.x;
  const isMultiSeries = spec.series.length > 1;
  const data = (spec.series[0]?.data || []).map((v, i) => {
    const point: Record<string, any> = { _idx: i, value: v };
    if (isMultiSeries) {
      spec.series.forEach((s) => {
        point[s.name] = s.data[i] ?? 0;
      });
    }
    return point;
  });

  const pieData = (spec.series[0]?.data || []).map((v, i) => ({
    name:
      typeof xLabels === 'string'
        ? `point ${i}`
        : (Array.isArray(xLabels) ? xLabels[i] : undefined) ?? `point ${i}`,
    value: v,
  }));

  const height = 220;
  const darkTheme = {
    background: '#0f172a',
    border: '1px solid #334155',
    borderRadius: 4,
    fontSize: 11,
  };

  const formatXTick = (v: any) =>
    typeof xLabels === 'string'
      ? v
      : Array.isArray(xLabels)
        ? (xLabels[v] ?? v)
        : v;

  return (
    <div className="my-2 rounded-md border border-rule bg-paper-2/60 p-3">
      {spec.title && (
        <div className="text-xs font-mono uppercase tracking-wider text-ink-2 mb-2">
          {spec.title}
        </div>
      )}
      <div style={{ width: '100%', height }}>
        <ResponsiveContainer>
          {spec.type === 'line' ? (
            <LineChart data={data}>
              <CartesianGrid stroke="#1e293b" strokeDasharray="3 3" />
              <XAxis
                dataKey="_idx"
                tick={{ fill: '#94a3b8', fontSize: 10 }}
                tickFormatter={formatXTick}
              />
              <YAxis tick={{ fill: '#94a3b8', fontSize: 10 }} />
              <Tooltip contentStyle={darkTheme} />
              <Legend wrapperStyle={{ fontSize: 10 }} />
              {spec.series.map((s, i) => (
                <Line
                  key={s.name}
                  type="monotone"
                  dataKey={isMultiSeries ? s.name : 'value'}
                  name={s.name}
                  stroke={COLORS[i % COLORS.length]}
                  strokeWidth={2}
                  dot={false}
                />
              ))}
            </LineChart>
          ) : spec.type === 'bar' ? (
            <BarChart data={data}>
              <CartesianGrid stroke="#1e293b" strokeDasharray="3 3" />
              <XAxis
                dataKey="_idx"
                tick={{ fill: '#94a3b8', fontSize: 10 }}
                tickFormatter={formatXTick}
              />
              <YAxis tick={{ fill: '#94a3b8', fontSize: 10 }} />
              <Tooltip contentStyle={darkTheme} />
              <Legend wrapperStyle={{ fontSize: 10 }} />
              {spec.series.map((s, i) => (
                <Bar
                  key={s.name}
                  dataKey={isMultiSeries ? s.name : 'value'}
                  name={s.name}
                  fill={COLORS[i % COLORS.length]}
                />
              ))}
            </BarChart>
          ) : spec.type === 'area' ? (
            <AreaChart data={data}>
              <CartesianGrid stroke="#1e293b" strokeDasharray="3 3" />
              <XAxis
                dataKey="_idx"
                tick={{ fill: '#94a3b8', fontSize: 10 }}
                tickFormatter={formatXTick}
              />
              <YAxis tick={{ fill: '#94a3b8', fontSize: 10 }} />
              <Tooltip contentStyle={darkTheme} />
              <Legend wrapperStyle={{ fontSize: 10 }} />
              {spec.series.map((s, i) => (
                <Area
                  key={s.name}
                  type="monotone"
                  dataKey={isMultiSeries ? s.name : 'value'}
                  name={s.name}
                  stroke={COLORS[i % COLORS.length]}
                  fill={COLORS[i % COLORS.length]}
                  fillOpacity={0.3}
                />
              ))}
            </AreaChart>
          ) : (
            <PieChart>
              <Tooltip contentStyle={darkTheme} />
              <Legend wrapperStyle={{ fontSize: 10 }} />
              <Pie
                data={pieData}
                dataKey="value"
                nameKey="name"
                outerRadius={80}
                label={{ fontSize: 10, fill: '#cbd5e1' }}
              >
                {pieData.map((_, i) => (
                  <Cell key={i} fill={COLORS[i % COLORS.length]} />
                ))}
              </Pie>
            </PieChart>
          )}
        </ResponsiveContainer>
      </div>
    </div>
  );
}