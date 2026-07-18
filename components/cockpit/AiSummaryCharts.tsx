'use client';

import React, { useState } from 'react';
import { ChartRenderer } from '@/components/chat/ChartRenderer';
import { PinToCockpitButton } from '@/components/chat/PinToCockpitButton';
import { T } from '@/components/i18n/T';
import type { ChartSpec } from '@/components/chat/chartContract';

export interface AiSummaryChartsProps {
  actor: { id: number; fullname?: string; role_name?: string };
  brief: any;
  stuck: Array<{ waybill_id: string; vendor_name: string; total_amount: string; current_stage: string; age_hours: string }>;
  locale?: 'en' | 'th' | 'de';
}

interface SummaryResponse {
  ok: boolean;
  text?: string;
  charts?: ChartSpec[];
  extracts?: any[];
  error?: string;
  modelName?: string;
  latencyMs?: number;
}

type Mode = 'idle' | 'loading' | 'loaded' | 'error';

function buildPayload(brief: any, stuck: AiSummaryChartsProps['stuck']) {
  const lines: string[] = [];
  lines.push('KPIs:');
  lines.push(JSON.stringify(brief?.kpis ?? {}, null, 0));
  lines.push('cashTrend (7d, day, cash):');
  lines.push(JSON.stringify(brief?.cashTrend ?? [], null, 0));
  lines.push('mtdTrend (7d, day, mtd):');
  lines.push(JSON.stringify(brief?.mtdTrend ?? [], null, 0));
  lines.push('deptBudgets:');
  lines.push(JSON.stringify(brief?.deptBudgets ?? [], null, 0));
  lines.push(`stuckCount: ${brief?.stuckCount ?? stuck.length}`);
  lines.push('stuck waybills:');
  lines.push(JSON.stringify(stuck ?? [], null, 0));
  lines.push('arAging (sales):');
  lines.push(JSON.stringify(brief?.arAging ?? [], null, 0));
  lines.push('salesTrend (last 7 days, day, total):');
  lines.push(JSON.stringify(brief?.salesTrend ?? [], null, 0));
  lines.push('topCustomers (revenue this month):');
  lines.push(JSON.stringify(brief?.topCustomers ?? [], null, 0));
  return lines.join('\n');
}

export function AiSummaryCharts({ actor, brief, stuck, locale = 'th' }: AiSummaryChartsProps) {
  const [mode, setMode] = useState<Mode>('idle');
  const [text, setText] = useState<string>('');
  const [charts, setCharts] = useState<ChartSpec[]>([]);
  const [error, setError] = useState<string>('');
  const [collapsed, setCollapsed] = useState<boolean>(false);
  const [meta, setMeta] = useState<{ modelName?: string; latencyMs?: number }>({});

  async function generate() {
    setMode('loading');
    setError('');
    try {
      const content = buildPayload(brief, stuck);
      const r = await fetch('/api/cockpit/summarize', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          actor,
          brief,
          stuck,
          lang: locale,
          messages: [{ role: 'user', content }],
        }),
      });
      const data: SummaryResponse = await r.json();
      if (!data.ok) {
        setError(data.error || 'AI call failed');
        setMode('error');
        return;
      }
      setText((data.text || '').replace(/\[CHART\][\s\S]*?\[\/CHART\]/g, '').trim());
      setCharts(data.charts || []);
      setMeta({ modelName: data.modelName, latencyMs: data.latencyMs });
      setMode('loaded');
      setCollapsed(false);
    } catch (e: any) {
      setError(e?.message || 'network error');
      setMode('error');
    }
  }

  function collapse() {
    setCollapsed(true);
    setMode('idle');
  }

  function expand() {
    setCollapsed(false);
    setMode('loaded');
  }

  return (
    <section className="rounded-md border border-accent bg-paper-2/50 p-4 sm:p-6">
      <div className="flex items-center justify-between gap-3 mb-3">
        <div className="text-xs font-mono font-bold uppercase text-accent tracking-wider">
          <T id="cockpit.aiSummaryHeader" />
        </div>
        <div className="flex items-center gap-2">
          {mode === 'loaded' && (
            <>
              <span className="text-xs font-mono text-mute">
                {meta.modelName ? `${meta.modelName}` : ''}{meta.latencyMs != null ? ` · ${meta.latencyMs}ms` : ''}
              </span>
              <button
                type="button"
                onClick={collapse}
                className="rounded-md border border-rule bg-paper hover:bg-paper-2 text-ink-2 text-xs font-bold px-3 py-2"
              >
                <T id="cockpit.collapse" />
              </button>
              <button
                type="button"
                onClick={generate}
                className="rounded-md bg-accent-strong hover:bg-accent disabled:opacity-50 text-ink text-xs font-bold px-4 py-2"
              >
                <T id="cockpit.regenerate" />
              </button>
            </>
          )}
          {mode === 'error' && (
            <button
              type="button"
              onClick={generate}
              className="rounded-md bg-accent-strong hover:bg-accent disabled:opacity-50 text-ink text-xs font-bold px-4 py-2"
            >
              <T id="common.retry" />
            </button>
          )}
          {mode === 'idle' && collapsed && (
            <button
              type="button"
              onClick={expand}
              className="rounded-md border border-rule bg-paper hover:bg-paper-2 text-ink-2 text-xs font-bold px-3 py-2"
            >
              <T id="cockpit.expand" />
            </button>
          )}
        </div>
      </div>

      {mode === 'idle' && !collapsed && (
        <button
          type="button"
          onClick={generate}
          className="w-full rounded-md border border-accent bg-accent-strong hover:bg-accent-strong text-accent-soft text-sm font-bold py-8 flex items-center justify-center gap-2"
        >
          <span className="text-2xl">✨</span>
          <span>
            <T id="cockpit.generateSummary" />
          </span>
        </button>
      )}

      {mode === 'idle' && collapsed && (
        <div className="text-xs font-mono text-mute">
          <T id="cockpit.summaryHidden" />
        </div>
      )}

      {mode === 'loading' && (
        <div className="flex items-center justify-center gap-3 py-12 text-ink-2">
          <span className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-accent border-t-transparent" />
          <span className="text-xs font-mono">
            <T id="cockpit.thinking" />
          </span>
        </div>
      )}

      {mode === 'error' && (
        <div className="rounded-md border border-critical bg-critical-soft p-4 text-critical-soft text-xs font-mono">
          {error || <T id="ai.explain.failed" />}
        </div>
      )}

      {mode === 'loaded' && (
        <div>
          {text && (
            <div className="rounded-md border border-rule bg-paper-2/60 p-4 mb-4 text-sm text-ink leading-relaxed whitespace-pre-wrap">
              {text}
            </div>
          )}
          {charts.length === 0 && (
            <div className="text-xs font-mono text-mute">
              <T id="cockpit.noCharts" />
            </div>
          )}
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
            {charts.map((c, i) => (
              <div key={i}>
                <ChartRenderer spec={c} />
                <div className="mt-2">
                  <PinToCockpitButton spec={c} tileId="cockpit" />
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </section>
  );
}