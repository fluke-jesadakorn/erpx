'use client';

import React, { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';

export interface AiCoaChipProps {
  itemId: number;
  expenseId: number;
  description: string;
  currentCode: string | null;
  currentScore: number | null;
  locale?: 'en' | 'th' | 'de';
  origin?: 'expense' | 'so';
  soId?: number;
  waybillId?: string;
}

export interface CoaSuggestion {
  code: string;
  name: string | null;
  name_th: string | null;
  normal_side: 'debit' | 'credit';
  similarity: number;
}

type State =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'ready'; suggestions: CoaSuggestion[] }
  | { kind: 'error' };

function clamp01(n: number): number {
  if (!Number.isFinite(n)) return 0;
  if (n < 0) return 0;
  if (n > 1) return 1;
  return n;
}

function similarityTone(s: number): {
  row: string;
  bar: string;
  glyph: string;
} {
  if (s >= 0.7) {
    return {
      row: 'border-emerald-500/40 bg-emerald-950/30 text-emerald-100',
      bar: 'bg-emerald-400',
      glyph: '✓',
    };
  }
  if (s >= 0.4) {
    return {
      row: 'border-amber-500/40 bg-amber-950/30 text-amber-100',
      bar: 'bg-amber-400',
      glyph: '~',
    };
  }
  return {
    row: 'border-rose-500/40 bg-rose-950/30 text-rose-100',
    bar: 'bg-rose-400',
    glyph: '!',
  };
}

function pickName(s: CoaSuggestion, locale: 'en' | 'th' | 'de'): string {
  if (locale === 'th' && s.name_th) return s.name_th;
  return s.name ?? s.code;
}

export function AiCoaChip({
  itemId,
  expenseId,
  description,
  currentCode,
  currentScore,
  locale = 'en',
  origin = 'expense',
  soId,
  waybillId,
}: AiCoaChipProps): React.JSX.Element {
  void itemId;
  void expenseId;
  void soId;
  void waybillId;
  const router = useRouter();
  const [state, setState] = useState<State>({ kind: 'idle' });
  const [applyingCode, setApplyingCode] = useState<string | null>(null);
  const [applyError, setApplyError] = useState<string | null>(null);

  useEffect(() => {
    if (origin === 'so') {
      if (!soId) {
        setState({ kind: 'idle' });
        return;
      }
      let cancelled = false;
      async function loadSo() {
        setState({ kind: 'loading' });
        try {
          const res = await fetch('/api/sales/coa-suggestions', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ soId }),
          });
          const data = (await res.json()) as {
            success?: boolean;
            suggestions?: CoaSuggestion[];
            error?: string;
          };
          if (cancelled) return;
          const list = Array.isArray(data?.suggestions) ? data.suggestions : [];
          setState({ kind: 'ready', suggestions: list });
        } catch {
          if (!cancelled) setState({ kind: 'error' });
        }
      }
      void loadSo();
      return () => {
        cancelled = true;
      };
    }

    if (!description || !description.trim()) {
      setState({ kind: 'idle' });
      return;
    }
    let cancelled = false;
    async function load() {
      setState({ kind: 'loading' });
      try {
        const res = await fetch('/api/waybill/coa-suggestions', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ description }),
        });
        const data = (await res.json()) as {
          success?: boolean;
          suggestions?: CoaSuggestion[];
          error?: string;
        };
        if (cancelled) return;
        const list = Array.isArray(data?.suggestions) ? data.suggestions : [];
        setState({ kind: 'ready', suggestions: list });
      } catch {
        if (!cancelled) setState({ kind: 'error' });
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [origin, description, soId]);

  const hasHighConfidenceExisting =
    !!currentCode && currentScore != null && clamp01(Number(currentScore)) >= 0.6;

  if (hasHighConfidenceExisting) {
    const pct = Math.round(clamp01(Number(currentScore ?? 0)) * 100);
    return (
      <span
        role="status"
        aria-live="polite"
        className="inline-flex items-center gap-1.5 rounded-lg border border-emerald-500/40 bg-emerald-950/30 px-2.5 py-1.5 font-mono text-xs text-emerald-100"
        title={`${currentCode} (${pct}% confidence)`}
      >
        <span aria-hidden className="font-bold">✓</span>
        <span className="font-bold uppercase tracking-wider">{currentCode}</span>
        <span className="opacity-80">·</span>
        <span className="opacity-90">{pct}%</span>
      </span>
    );
  }

  if (state.kind === 'idle') {
    return (
      <span className="inline-flex items-center gap-1.5 rounded-lg border border-slate-700/60 bg-slate-900/40 px-2.5 py-1 font-mono text-xs text-slate-400">
        <span aria-hidden>🪙</span>
        <span className="uppercase tracking-wider">COA · pending description</span>
      </span>
    );
  }

  if (state.kind === 'loading') {
    return (
      <span className="inline-flex items-center gap-1.5 rounded-lg border border-slate-700/60 bg-slate-900/40 px-2.5 py-1 font-mono text-xs text-slate-400">
        <span aria-hidden>🪙</span>
        <span className="uppercase tracking-wider">COA · matching…</span>
      </span>
    );
  }

  if (state.kind === 'error') {
    return (
      <span className="inline-flex items-center gap-1.5 rounded-lg border border-slate-700/60 bg-slate-900/40 px-2.5 py-1 font-mono text-xs text-slate-400">
        <span aria-hidden>🪙</span>
        <span className="uppercase tracking-wider">COA · unavailable</span>
      </span>
    );
  }

  if (state.suggestions.length === 0) {
    return (
      <span className="inline-flex items-center gap-1.5 rounded-lg border border-slate-700/60 bg-slate-900/40 px-2.5 py-1 font-mono text-xs text-slate-400">
        <span aria-hidden>🪙</span>
        <span className="uppercase tracking-wider">COA · no matches</span>
      </span>
    );
  }

  const onApply = async (s: CoaSuggestion) => {
    setApplyingCode(s.code);
    setApplyError(null);
    try {
      const res = origin === 'so'
        ? await fetch('/api/sales/apply-coa-suggestion', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ soId, itemId, code: s.code, waybillId }),
          })
        : await fetch('/api/waybill/apply-coa-suggestion', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              itemId,
              code: s.code,
              normalSide: s.normal_side,
              expenseId,
            }),
          });
      const data = (await res.json()) as { ok?: boolean; error?: string };
      if (!res.ok || !data?.ok) {
        setApplyError(data?.error ?? `HTTP ${res.status}`);
        return;
      }
      router.refresh();
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setApplyError(msg);
    } finally {
      setApplyingCode(null);
    }
  };

  return (
    <div
      role="region"
      aria-label="AI chart-of-account suggestions"
      className="inline-flex max-w-md flex-col gap-1.5 rounded-2xl border border-slate-700/60 bg-slate-950/70 p-2 font-mono text-xs"
    >
      <div className="flex items-center gap-2 px-1 text-slate-400">
        <span aria-hidden>🪙</span>
        <span className="font-bold uppercase tracking-wider text-slate-300">AI COA</span>
        <span className="opacity-60">·</span>
        <span className="uppercase tracking-widest opacity-70">top matches</span>
        {currentCode && !hasHighConfidenceExisting && (
          <span className="ml-auto rounded-full border border-slate-700 bg-slate-900 px-2 py-0.5 text-[10px] uppercase tracking-widest text-slate-400">
            current: {currentCode}
          </span>
        )}
      </div>

      <ul className="flex flex-col gap-1">
        {state.suggestions.map((s) => {
          const sim = clamp01(Number(s.similarity) / 100);
          const pct = Math.round(sim * 100);
          const tone = similarityTone(sim);
          return (
            <li
              key={s.code}
              className={`flex items-center gap-2 rounded-lg border px-2 py-1.5 ${tone.row}`}
            >
              <span aria-hidden className="w-3 text-center font-bold">{tone.glyph}</span>
              <span className="font-bold">{s.code}</span>
              <span className="truncate opacity-90" title={s.name ?? ''}>
                {pickName(s, locale)}
              </span>
              <span className="opacity-70">·</span>
              <span className="uppercase tracking-widest opacity-80">{s.normal_side}</span>
              <div className="ml-1 flex w-20 items-center gap-1.5">
                <div className="h-1 flex-1 overflow-hidden rounded-full bg-black/40">
                  <div className={`h-full ${tone.bar}`} style={{ width: `${pct}%` }} aria-label={`similarity ${pct}%`} />
                </div>
                <span className="w-9 text-right font-bold">{pct}%</span>
              </div>
              <button
                type="button"
                onClick={() => void onApply(s)}
                disabled={applyingCode !== null}
                className="ml-auto rounded-md border border-cyan-500/40 bg-cyan-500/15 px-2 py-0.5 font-bold uppercase tracking-wider text-cyan-200 hover:bg-cyan-500/30 disabled:opacity-50"
              >
                {applyingCode === s.code ? '…' : 'Apply'}
              </button>
            </li>
          );
        })}
      </ul>

      {applyError && (
        <div className="rounded border border-rose-500/50 bg-rose-950/40 px-2 py-1 text-rose-200">
          {applyError}
        </div>
      )}
    </div>
  );
}

export default AiCoaChip;
