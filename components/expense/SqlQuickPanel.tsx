'use client';
import { useState } from 'react';
import { SqlResultTable } from '@/components/chat/SqlResultTable';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';

interface QuickPrompt {
  icon: string;
  label: string;
  labelTh?: string;
  labelDe?: string;
  question: string;
}

const PROMPTS: QuickPrompt[] = [
  {
    icon: '📤',
    label: 'My pending',
    labelTh: 'ของฉันที่รออนุมัติ',
    labelDe: 'Meine offenen',
    question:
      'List my pending expense waybills with vendor, amount, and current stage. Order by created_at desc. Limit 50.',
  },
  {
    icon: '📊',
    label: 'MTD by category',
    labelTh: 'ยอดเดือนนี้ตามหมวด',
    labelDe: 'MTD nach Kategorie',
    question:
      'Sum of expense amounts this month grouped by category. Limit 20.',
  },
  {
    icon: '🏪',
    label: 'Top 5 vendors (this quarter)',
    labelTh: 'Top 5 ผู้ขาย (ไตรมาสนี้)',
    labelDe: 'Top 5 Lieferanten (Quartal)',
    question:
      'Top 5 vendors by total expense amount this fiscal quarter. Limit 5.',
  },
  {
    icon: '⏳',
    label: 'Stuck > 24h',
    labelTh: 'ค้างเกิน 24 ชม.',
    labelDe: 'Hängend > 24h',
    question:
      'Waybills that have not changed stage in over 24 hours, with current stage and age in hours. Limit 50.',
  },
];

interface SqlApiResult {
  ok: boolean;
  sql?: string;
  columns?: string[];
  rows?: Array<Record<string, unknown>>;
  rowCount?: number;
  explanation?: string;
  error?: string;
}

interface PanelResult {
  label: string;
  data: SqlApiResult | null;
  error?: string;
}

export function SqlQuickPanel() {
  const locale = useSecondaryLocale();
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<PanelResult[]>([]);
  const labelFor = (p: QuickPrompt) =>
    locale === 'th' ? (p.labelTh ?? p.label) : locale === 'de' ? (p.labelDe ?? p.label) : p.label;

  async function run(p: QuickPrompt) {
    setBusy(true);
    try {
      const r = await fetch('/api/ai/sql', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ question: p.question, lang: locale }),
      });
      const data = (await r.json()) as SqlApiResult;
      setResults((rs) => [{ label: labelFor(p), data }, ...rs].slice(0, 4));
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setResults((rs) => [{ label: labelFor(p), data: null, error: msg }, ...rs].slice(0, 4));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="mb-6 rounded-md border border-info-strong bg-info-soft p-4">
      <header className="mb-3 flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-info">
        <span aria-hidden>🔍</span>
        <span>Quick SQL · read-only</span>
      </header>
      <div className="flex flex-wrap gap-2">
        {PROMPTS.map((p, i) => (
          <button
            key={i}
            type="button"
            onClick={() => run(p)}
            disabled={busy}
            className="rounded-full border border-info-strong bg-info px-3 py-1.5 text-xs font-mono text-paper hover:bg-info disabled:opacity-50"
          >
            <span aria-hidden className="mr-1">
              {p.icon}
            </span>
            {labelFor(p)}
          </button>
        ))}
      </div>
      <div className="mt-4 space-y-3">
        {results.map((r, i) => (
          <div key={i}>
            <div className="mb-1 text-xs font-mono text-ink-2">{r.label}</div>
            {r.error && (
              <div className="rounded border border-critical-strong bg-critical-soft p-2 text-xs text-critical">
                ⚠️ {r.error}
              </div>
            )}
            {r.data?.ok && (
              <SqlResultTable
                sql={r.data.sql ?? ''}
                columns={r.data.columns ?? []}
                rows={r.data.rows ?? []}
                rowCount={r.data.rowCount ?? 0}
                explanation={r.data.explanation}
              />
            )}
            {r.data && !r.data.ok && (
              <div className="rounded border border-caution-strong bg-caution-soft p-2 text-xs text-caution">
                {r.data.error || 'AI could not generate SQL'}
              </div>
            )}
          </div>
        ))}
      </div>
    </section>
  );
}
