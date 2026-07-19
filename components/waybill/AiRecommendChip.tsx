'use client';

import React, { useState } from 'react';
import { T } from '@/components/i18n/T';

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

type State =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'ready'; data: RecommendOk }
  | { kind: 'error' };

function decisionTone(decision: 'approve' | 'reject'): string {
  return decision === 'approve'
    ? 'border-positive/50 bg-positive-soft text-positive'
    : 'border-critical/50 bg-critical-soft text-critical';
}

function confidenceBarTone(confidence: number): string {
  if (confidence >= 0.7) return 'bg-positive';
  if (confidence >= 0.4) return 'bg-caution';
  return 'bg-critical';
}

function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(1, value));
}

export function AiRecommendChip({ waybillId, amount, currentStage, vendorName }: Props) {
  const [state, setState] = useState<State>({ kind: 'idle' });

  async function load() {
    if (state.kind === 'loading') return;
    setState({ kind: 'loading' });
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
      const data = (await res.json()) as RecommendOk | { ok: false };
      if (!res.ok || !data.ok) {
        setState({ kind: 'error' });
        return;
      }
      setState({
        kind: 'ready',
        data: {
          ...data,
          decision: data.decision === 'reject' ? 'reject' : 'approve',
          confidence: clamp01(Number(data.confidence)),
          rationale: data.rationale || '',
          modelName: data.modelName || 'unknown',
          latencyMs: data.latencyMs ?? 0,
        },
      });
    } catch {
      setState({ kind: 'error' });
    }
  }

  if (state.kind === 'idle') {
    return (
      <button
        type="button"
        onClick={() => void load()}
        className="inline-flex min-h-10 items-center gap-2 rounded-lg border border-accent/50 bg-accent-soft px-3 py-2 text-xs font-semibold text-accent-ink transition hover:border-accent hover:bg-accent/15"
      >
        <span aria-hidden>✦</span>
        <T id="waybill.ai.recommend" />
      </button>
    );
  }

  if (state.kind === 'loading') {
    return (
      <div className="inline-flex min-h-10 items-center gap-2 rounded-lg border border-rule bg-paper-2 px-3 py-2 text-xs text-ink-2" role="status" aria-live="polite">
        <span aria-hidden>✦</span>
        <T id="waybill.ai.thinkingLoading" />
      </div>
    );
  }

  if (state.kind === 'error') {
    return (
      <button
        type="button"
        onClick={() => void load()}
        className="inline-flex min-h-10 items-center gap-2 rounded-lg border border-critical/50 bg-critical-soft px-3 py-2 text-xs font-semibold text-critical transition hover:border-critical"
      >
        <span aria-hidden>!</span>
        <T id="waybill.ai.unavailable" />
      </button>
    );
  }

  const { decision, confidence, rationale, modelName, latencyMs } = state.data;
  const pct = Math.round(confidence * 100);
  return (
    <div className={`w-full rounded-lg border p-3 ${decisionTone(decision)}`} role="status" aria-live="polite">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-xs font-semibold uppercase tracking-[0.14em]">
          <T id="waybill.ai.suggests" />: {decision === 'approve' ? '✓' : '✕'} {pct}%
        </p>
        <button type="button" onClick={() => void load()} className="text-xs underline-offset-2 hover:underline">
          <T id="waybill.review.refresh" />
        </button>
      </div>
      <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-paper-3">
        <div className={`h-full ${confidenceBarTone(confidence)}`} style={{ width: `${pct}%` }} />
      </div>
      {rationale && <p className="mt-2 text-sm leading-relaxed">{rationale}</p>}
      <p className="mt-2 text-[11px] font-mono uppercase tracking-wider opacity-70">
        {modelName} · {latencyMs}ms · <T id="waybill.ai.neverAutoClicks" />
      </p>
    </div>
  );
}

export default AiRecommendChip;
