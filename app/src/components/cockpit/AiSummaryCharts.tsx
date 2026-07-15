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

  const headerTitle = { en: '🤖 AI Summary · Charts & Insights', th: '🤖 สรุป AI · แผนภูมิและข้อมูลเชิงลึก', de: '🤖 KI-Zusammenfassung · Diagramme & Erkenntnisse' };

  return (
    <section className="rounded-3xl border border-indigo-500/30 bg-slate-950/40 p-4 sm:p-6">
      <div className="flex items-center justify-between gap-3 mb-3">
        <div className="text-xs font-mono font-bold uppercase text-indigo-300 tracking-wider">
          <T value={headerTitle} />
        </div>
        <div className="flex items-center gap-2">
          {mode === 'loaded' && (
            <>
              <span className="text-xs font-mono text-slate-500">
                {meta.modelName ? `${meta.modelName}` : ''}{meta.latencyMs != null ? ` · ${meta.latencyMs}ms` : ''}
              </span>
              <button
                type="button"
                onClick={collapse}
                className="rounded-xl border border-slate-700 bg-slate-900 hover:bg-slate-800 text-slate-300 text-xs font-bold px-3 py-2"
              >
                <T value={{ en: 'Collapse', th: 'ย่อ', de: 'Einklappen' }} />
              </button>
              <button
                type="button"
                onClick={generate}
                className="rounded-xl bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white text-xs font-bold px-4 py-2"
              >
                <T value={{ en: 'Regenerate', th: 'สร้างใหม่', de: 'Neu generieren' }} />
              </button>
            </>
          )}
          {mode === 'error' && (
            <button
              type="button"
              onClick={generate}
              className="rounded-xl bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white text-xs font-bold px-4 py-2"
            >
              <T value={{ en: 'Retry', th: 'ลองใหม่', de: 'Erneut versuchen' }} />
            </button>
          )}
          {mode === 'idle' && collapsed && (
            <button
              type="button"
              onClick={expand}
              className="rounded-xl border border-slate-700 bg-slate-900 hover:bg-slate-800 text-slate-300 text-xs font-bold px-3 py-2"
            >
              <T value={{ en: 'Expand', th: 'ขยาย', de: 'Ausklappen' }} />
            </button>
          )}
        </div>
      </div>

      {mode === 'idle' && !collapsed && (
        <button
          type="button"
          onClick={generate}
          className="w-full rounded-2xl border border-indigo-500/40 bg-indigo-950/30 hover:bg-indigo-900/40 text-indigo-200 text-sm font-bold py-8 flex items-center justify-center gap-2"
        >
          <span className="text-2xl">✨</span>
          <span>
            <T value={{ en: 'Generate summary', th: 'สร้างสรุป', de: 'Zusammenfassung erstellen' }} />
          </span>
        </button>
      )}

      {mode === 'idle' && collapsed && (
        <div className="text-xs font-mono text-slate-500">
          <T value={{ en: 'Summary hidden. Click Expand to view again.', th: 'ซ่อนสรุปไว้ กดขยายเพื่อดูอีกครั้ง', de: 'Zusammenfassung ausgeblendet. Auf Ausklappen klicken, um sie erneut anzuzeigen.' }} />
        </div>
      )}

      {mode === 'loading' && (
        <div className="flex items-center justify-center gap-3 py-12 text-slate-300">
          <span className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-indigo-400 border-t-transparent" />
          <span className="text-xs font-mono">
            <T value={{ en: 'Thinking…', th: 'กำลังคิด…', de: 'Denkt nach…' }} />
          </span>
        </div>
      )}

      {mode === 'error' && (
        <div className="rounded-xl border border-rose-500/40 bg-rose-950/30 p-4 text-rose-200 text-xs font-mono">
          {error || (
            <T value={{ en: 'AI call failed', th: 'การเรียก AI ล้มเหลว', de: 'KI-Aufruf fehlgeschlagen' }} />
          )}
        </div>
      )}

      {mode === 'loaded' && (
        <div>
          {text && (
            <div className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4 mb-4 text-sm text-slate-200 leading-relaxed whitespace-pre-wrap">
              {text}
            </div>
          )}
          {charts.length === 0 && (
            <div className="text-xs font-mono text-slate-500">
              <T value={{ en: 'No charts returned by the model.', th: 'โมเดลไม่ส่งแผนภูมิกลับมา', de: 'Modell hat keine Diagramme zurückgegeben.' }} />
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