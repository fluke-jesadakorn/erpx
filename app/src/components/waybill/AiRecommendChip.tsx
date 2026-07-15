'use client';

import React, { useEffect, useState } from 'react';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { Bilingual } from '@/components/i18n/Bilingual';
import type { BilingualText } from '@folio-lib/i18n/types';

interface Props {
  waybillId: string;
  amount?: number | string | null;
  currentStage: string;
  vendorName?: string | null;
}

interface RecommendOk {
  ok: true;
  decision: 'approve' | 'reject';
  confidence: number;
  rationale: string;
  modelName: string;
  latencyMs: number;
}

interface RecommendFail {
  ok: false;
  error?: string;
}

type State =
  | { kind: 'loading' }
  | { kind: 'ready'; data: RecommendOk }
  | { kind: 'error' };

function decisionTone(d: 'approve' | 'reject'): string {
  return d === 'approve'
    ? 'border-emerald-500/40 bg-emerald-950/30 text-emerald-100'
    : 'border-rose-500/40 bg-rose-950/30 text-rose-100';
}

function decisionGlyph(d: 'approve' | 'reject'): BilingualText {
  return d === 'approve'
    ? { en: '✅ APPROVE', th: '✅ อนุมัติ', de: '✅ GENEHMIGEN' }
    : { en: '❌ REJECT', th: '❌ ปฏิเสธ', de: '❌ ABLEHNEN' };
}

function confidenceBarTone(c: number): string {
  if (c >= 0.7) return 'bg-emerald-400';
  if (c >= 0.4) return 'bg-amber-400';
  return 'bg-rose-400';
}

function clamp01(n: number): number {
  if (!Number.isFinite(n)) return 0;
  if (n < 0) return 0;
  if (n > 1) return 1;
  return n;
}

export function AiRecommendChip({
  waybillId,
  amount,
  currentStage,
  vendorName,
}: Props) {
  const locale = useSecondaryLocale();
  const [state, setState] = useState<State>({ kind: 'loading' });

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const res = await fetch('/api/waybill/recommend', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            waybillId,
            amount: amount == null ? null : Number(amount),
            currentStage,
            vendorName: vendorName ?? null,
          }),
        });
        const data = (await res.json()) as RecommendOk | RecommendFail;
        if (cancelled) return;
        if (data && (data as RecommendOk).ok) {
          const ok = data as RecommendOk;
          const conf = clamp01(Number(ok.confidence));
          const decision: 'approve' | 'reject' = ok.decision === 'reject' ? 'reject' : 'approve';
          setState({
            kind: 'ready',
            data: {
              ok: true,
              decision,
              confidence: conf,
              rationale: ok.rationale || '',
              modelName: ok.modelName || 'unknown',
              latencyMs: ok.latencyMs ?? 0,
            },
          });
        } else {
          setState({ kind: 'error' });
        }
      } catch {
        if (!cancelled) setState({ kind: 'error' });
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [waybillId, amount, currentStage, vendorName]);

  if (state.kind === 'loading') {
    return (
      <div className="inline-flex items-center gap-1.5 rounded-lg border border-slate-700/60 bg-slate-900/40 px-2.5 py-1 font-mono text-xs text-slate-400">
        <span aria-hidden>🤖</span>
        <Bilingual en="AI thinking…" th="AI กำลังคิด…" de="KI denkt nach…" locale={locale} />
      </div>
    );
  }

  if (state.kind === 'error') {
    return (
      <div className="inline-flex items-center gap-1.5 rounded-lg border border-slate-700/60 bg-slate-900/40 px-2.5 py-1 font-mono text-xs text-slate-400">
        <span aria-hidden>🤖</span>
        <Bilingual en="AI unavailable" th="AI ไม่พร้อมใช้งาน" de="KI nicht verfügbar" locale={locale} />
      </div>
    );
  }

  const { decision, confidence, rationale, modelName, latencyMs } = state.data;
  const pct = Math.round(confidence * 100);
  const tone = decisionTone(decision);
  const barTone = confidenceBarTone(confidence);

  return (
    <div
      role="status"
      aria-live="polite"
      className={`inline-flex max-w-sm flex-col gap-1 rounded-lg border px-2.5 py-1.5 font-mono text-xs ${tone}`}
    >
      <div className="flex items-center gap-2">
        <span aria-hidden>🤖</span>
        <span className="font-bold uppercase tracking-wider">
          <Bilingual en="AI suggests" th="AI แนะนำ" de="KI empfiehlt" locale={locale} />:
        </span>
        <span className="font-bold">
          <Bilingual {...decisionGlyph(decision)} locale={locale} />
        </span>
        <span className="font-bold">{pct}%</span>
      </div>

      <div className="flex items-center gap-1.5">
        <div className="h-1 w-24 overflow-hidden rounded-full bg-black/40">
          <div
            className={`h-full ${barTone}`}
            style={{ width: `${pct}%` }}
            aria-label={`confidence ${pct}%`}
          />
        </div>
        <span className="text-xs uppercase tracking-widest opacity-70">
          <Bilingual en="confidence" th="ความมั่นใจ" de="Konfidenz" locale={locale} />
        </span>
      </div>

      {rationale && (
        <p className="font-sans text-sm leading-snug opacity-90">{rationale}</p>
      )}

      <div className="font-mono text-xs uppercase tracking-widest opacity-60">
        model: {modelName} · {latencyMs}ms ·{' '}
        <Bilingual en="never auto-clicks" th="ไม่กดอัตโนมัติ" de="klickt nie automatisch" locale={locale} />
      </div>
    </div>
  );
}

export default AiRecommendChip;
