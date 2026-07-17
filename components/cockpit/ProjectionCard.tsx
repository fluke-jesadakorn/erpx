'use client';

import React, { useEffect, useState } from 'react';
import {
  ResponsiveContainer, LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ReferenceLine, Legend,
} from 'recharts';
import { T } from '@/components/i18n/T';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';

interface ProjectionPoint {
  date: string;
  cash?: number | null;
  mtd?: number | null;
  cashProjected?: number | null;
  mtdProjected?: number | null;
  isProjected?: boolean;
}

interface ProjectionRegression {
  cash: { slope: number; intercept: number; r2: number };
  mtd: { slope: number; intercept: number; r2: number };
}

interface ProjectionSummary {
  currentCash: number;
  currentMtd: number;
  projectedCash30: number;
  projectedCash60: number;
  projectedCash90: number;
  monthlyBurn: number;
  daysToZero: number | null;
  trend: 'up' | 'down' | 'flat';
  r2: number;
}

interface ProjectionResult {
  historical: ProjectionPoint[];
  regression: ProjectionRegression;
  projection: ProjectionPoint[];
  summary: ProjectionSummary;
}

interface CombinedPoint {
  date: string;
  cash: number | null;
  cashProjected: number | null;
}

const TODAY = (() => {
  const d = new Date();
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
})();

function formatTHB(n: number): string {
  return (Number(n) || 0).toLocaleString('th-TH', { maximumFractionDigits: 0 });
}

function deltaColor(delta: number): string {
  return delta >= 0 ? 'text-emerald-400' : 'text-rose-400';
}

function deltaArrow(delta: number): string {
  return delta >= 0 ? '↑' : '↓';
}

export function ProjectionCard() {
  const locale = useSecondaryLocale();
  const [projection, setProjection] = useState<ProjectionResult | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [aiBusy, setAiBusy] = useState(false);
  const [aiText, setAiText] = useState<string | null>(null);
  const [aiError, setAiError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const r = await fetch('/api/cockpit/projection?days=90', { cache: 'no-store' });
        const data = await r.json();
        if (!alive) return;
        if (!data?.ok) {
          setError(data?.error || 'failed');
        } else {
          setProjection(data.projection as ProjectionResult);
        }
      } catch (e: any) {
        if (alive) setError(e?.message || 'network error');
      } finally {
        if (alive) setLoading(false);
      }
    })();
    return () => { alive = false; };
  }, []);

  async function handleAiInterpret() {
    if (!projection || aiBusy) return;
    setAiBusy(true);
    setAiError(null);
    setCopied(false);
    try {
      const summary = projection.summary;
      const userPrompt =
        `Current cash: ${formatTHB(summary.currentCash)} THB\n` +
        `MTD expenses: ${formatTHB(summary.currentMtd)} THB\n` +
        `Projected day 30: ${formatTHB(summary.projectedCash30)} THB\n` +
        `Projected day 60: ${formatTHB(summary.projectedCash60)} THB\n` +
        `Projected day 90: ${formatTHB(summary.projectedCash90)} THB\n` +
        `Monthly burn: ${formatTHB(summary.monthlyBurn)} THB\n` +
        `Days to zero: ${summary.daysToZero ?? 'N/A'}\n` +
        `Trend: ${summary.trend}\n` +
        `R² fit quality: ${summary.r2.toFixed(2)}\n` +
        `Regression slope: ${projection.regression.cash.slope.toFixed(2)} THB/day`;
      const r = await fetch('/api/ai/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          sectionKey: 'cockpit:projection',
          tileId: 'cockpit',
          messages: [{ role: 'user', content: userPrompt }],
          lang: locale,
        }),
      });
      const data = await r.json();
      if (!data?.ok) setAiError(data?.error || 'AI call failed');
      else setAiText(data.text || data.plain || '');
    } catch (e: any) {
      setAiError(e?.message || 'network error');
    } finally {
      setAiBusy(false);
    }
  }

  async function handleCopy() {
    if (!aiText) return;
    try {
      await navigator.clipboard.writeText(aiText);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {}
  }

  if (loading) {
    return (
      <section className="rounded-3xl border border-emerald-500/30 bg-slate-950/40 p-4 sm:p-6">
        <div className="flex items-center gap-2 text-sm font-mono text-slate-400">
          <span className="inline-block h-3 w-3 animate-spin rounded-full border-2 border-emerald-400 border-t-transparent" />
          <T id="cockpit.projectionComputing" />
        </div>
      </section>
    );
  }

  if (error || !projection) {
    return (
      <section className="rounded-3xl border border-emerald-500/30 bg-slate-950/40 p-4 sm:p-6">
        <div className="text-xs font-mono font-bold uppercase text-emerald-300 tracking-wider mb-2">
          <T id="cockpit.projectionTitle" />
        </div>
        <p className="text-xs font-mono text-slate-400">
          {error ? (
            <T id="cockpit.projectionError" values={{ error }} />
          ) : (
            <T id="cockpit.projectionNeedSeed" />
          )}
        </p>
      </section>
    );
  }

  const data: CombinedPoint[] = [
    ...projection.historical.map((p) => ({
      date: p.date,
      cash: typeof p.cash === 'number' ? p.cash : null,
      cashProjected: null,
    })),
    ...projection.projection.map((p) => ({
      date: p.date,
      cash: null,
      cashProjected: typeof p.cashProjected === 'number' ? p.cashProjected : null,
    })),
  ];

  const summary = projection.summary;
  const hasData = data.length > 0;
  const deltas = [
    { id: 'cockpit.projectionDay30', value: summary.projectedCash30 },
    { id: 'cockpit.projectionDay60', value: summary.projectedCash60 },
    { id: 'cockpit.projectionDay90', value: summary.projectedCash90 },
  ];

  return (
    <section className="rounded-3xl border border-emerald-500/30 bg-slate-950/40 p-4 sm:p-6">
      <div className="flex items-center justify-between mb-3">
        <div className="text-xs font-mono font-bold uppercase text-emerald-300 tracking-wider">
          <T id="cockpit.projectionTitle90" />
        </div>
        <div className="text-xs font-mono text-slate-500">
          <T id="cockpit.projectionFitQuality" values={{ r2: (summary.r2 || 0).toFixed(2) }} />
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <div className="lg:col-span-2 rounded-2xl border border-slate-800 bg-slate-950/60 p-3">
          {!hasData ? (
            <div className="flex h-[220px] sm:h-[280px] items-center justify-center">
              <p className="text-sm font-mono text-slate-500 text-center px-4">
                <T id="cockpit.projectionNeedSeed" />
              </p>
            </div>
          ) : (
            <div className="h-[220px] sm:h-[280px] w-full">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={data} margin={{ top: 12, right: 12, left: 0, bottom: 0 }}>
                  <CartesianGrid stroke="#1e293b" strokeDasharray="3 3" />
                  <XAxis
                    dataKey="date"
                    tick={{ fill: '#94a3b8', fontSize: 10 }}
                    tickFormatter={(d: string) => (typeof d === 'string' ? d.slice(5) : d)}
                  />
                  <YAxis
                    tick={{ fill: '#94a3b8', fontSize: 10 }}
                    tickFormatter={(v: number) => `${(v / 1000).toFixed(0)}k`}
                  />
                  <Tooltip
                    contentStyle={{ background: '#0f172a', border: '1px solid #334155', borderRadius: 4, fontSize: 11 }}
                    formatter={((value: unknown, name: unknown) => [
                      `${Number(value).toLocaleString()} THB`,
                      <T
                        key="series"
                        id={name === 'cash' ? 'cockpit.cashActual' : 'cockpit.cashProjected'}
                      />,
                    ]) as any}
                    labelStyle={{ color: '#94a3b8', fontSize: 10 }}
                  />
                  <Legend wrapperStyle={{ fontSize: 10 }} />
                  <ReferenceLine
                    x={TODAY}
                    stroke="#475569"
                    strokeDasharray="3 3"
                    label={{ value: 'Today', fill: '#94a3b8', fontSize: 10, position: 'top' }}
                  />
                  <Line
                    type="monotone"
                    dataKey="cash"
                    name="Cash (actual)"
                    stroke="#818cf8"
                    strokeWidth={2}
                    dot={false}
                    connectNulls={false}
                    isAnimationActive={false}
                  />
                  <Line
                    type="monotone"
                    dataKey="cashProjected"
                    name="Cash (projected)"
                    stroke="#f472b6"
                    strokeWidth={2}
                    strokeDasharray="5 5"
                    dot={false}
                    connectNulls={false}
                    isAnimationActive={false}
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
          )}
        </div>

        <div className="lg:col-span-1 space-y-2">
          <div className="rounded-2xl border border-slate-800 bg-slate-950/60 p-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-mono uppercase tracking-wider text-slate-400">
                <T id="cockpit.projectionCurrentCash" />
              </span>
              <span className="text-[14px] font-mono font-bold text-white">{formatTHB(summary.currentCash)}</span>
            </div>
          </div>
          {deltas.map((d) => {
            const delta = d.value - summary.currentCash;
            return (
              <div key={d.id} className="rounded-2xl border border-slate-800 bg-slate-950/60 p-3">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-mono uppercase tracking-wider text-slate-400">
                    <T id={d.id} />
                  </span>
                  <span className={`text-[14px] font-mono font-bold ${deltaColor(delta)}`}>
                    <span className="mr-1">{deltaArrow(delta)}</span>
                    {formatTHB(d.value)}
                  </span>
                </div>
              </div>
            );
          })}

          <div className="rounded-2xl border border-slate-800 bg-slate-950/60 p-3 text-sm font-mono">
            {summary.trend === 'down' && summary.daysToZero != null && (
              <div className="text-rose-400">
                🔥{' '}
                <T
                  id="cockpit.projectionDeclineWithDays"
                  values={{ rate: Math.abs(summary.monthlyBurn / 1000).toFixed(0), days: summary.daysToZero }}
                />
              </div>
            )}
            {summary.trend === 'down' && summary.daysToZero == null && (
              <div className="text-rose-400">
                🔥{' '}
                <T
                  id="cockpit.projectionDecline"
                  values={{ rate: Math.abs(summary.monthlyBurn / 1000).toFixed(0) }}
                />
              </div>
            )}
            {summary.trend === 'up' && (
              <div className="text-emerald-400">
                📈{' '}
                <T
                  id="cockpit.projectionGrowth"
                  values={{ rate: Math.abs(summary.monthlyBurn / 1000).toFixed(0) }}
                />
              </div>
            )}
            {summary.trend === 'flat' && (
              <div className="text-slate-400">
                <T id="cockpit.projectionFlat" />
              </div>
            )}
            <div className="text-xs text-slate-500 mt-1">
              <T id="cockpit.projectionFitInline" values={{ r2: summary.r2.toFixed(2) }} />
            </div>
          </div>
        </div>
      </div>

      <div className="mt-4 flex items-center gap-2">
        <button
          type="button"
          onClick={handleAiInterpret}
          disabled={aiBusy}
          className="inline-flex items-center gap-1.5 rounded-xl border border-emerald-500/40 bg-emerald-500/20 px-3 py-1.5 text-sm font-bold text-emerald-200 hover:bg-emerald-500/30 disabled:opacity-50 disabled:cursor-not-allowed transition-all"
        >
          <span>🤖</span>
          <span>
            <T id={aiBusy ? 'cockpit.interpreting' : 'cockpit.aiInterpret'} />
          </span>
        </button>
      </div>

      {(aiText || aiError || aiBusy) && (
        <div className="mt-3 p-3 rounded-xl border border-slate-800 bg-slate-950/60 text-sm text-slate-200">
          <div className="flex items-start justify-between gap-2">
            <div className="flex-1 whitespace-pre-wrap break-words">
              {aiBusy && (
                <span className="text-slate-500">
                  🤖 <T id="cockpit.thinking" />
                </span>
              )}
              {aiError && (
                <span className="text-rose-400">
                  ⚠ {aiError}
                </span>
              )}
              {aiText && !aiBusy && !aiError && aiText}
            </div>
            {aiText && !aiBusy && (
              <button
                type="button"
                onClick={handleCopy}
                className="shrink-0 rounded-md border border-slate-700 bg-slate-900 px-2 py-1 text-xs font-mono text-slate-300 hover:bg-slate-800"
              >
                <T id={copied ? 'ai.explain.copied' : 'cockpit.copy'} />
              </button>
            )}
          </div>
        </div>
      )}
    </section>
  );
}

export default ProjectionCard;