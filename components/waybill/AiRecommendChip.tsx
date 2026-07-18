'use client';

import React, { useEffect, useState } from 'react';
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
    ? 'border-positive bg-positive-strong text-positive-soft'
    : 'border-critical bg-critical-strong text-critical-soft';
}

function confidenceBarTone(c: number): string {
  if (c >= 0.7) return 'bg-positive';
  if (c >= 0.4) return 'bg-caution';
  return 'bg-critical';
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
      <div className="inline-flex items-center gap-1.5 rounded-lg border border-rule/60 bg-paper-2/40 px-2.5 py-1 font-mono text-xs text-ink-2">
        <span aria-hidden>🤖</span>
        <T id="waybill.ai.thinkingLoading" />
      </div>
    );
  }

  if (state.kind === 'error') {
    return (
      <div className="inline-flex items-center gap-1.5 rounded-lg border border-rule/60 bg-paper-2/40 px-2.5 py-1 font-mono text-xs text-ink-2">
        <span aria-hidden>🤖</span>
        <T id="waybill.ai.unavailable" />
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
          <T id="waybill.ai.suggests" />:
        </span>
        <span className="font-bold">
          <T id={decision === 'approve' ? 'waybill.ai.approveGlyph' : 'waybill.ai.rejectGlyph'} />
        </span>
        <span className="font-bold">{pct}%</span>
      </div>

      <div className="flex items-center gap-1.5">
        <div className="h-1 w-24 overflow-hidden rounded-full bg-paper-3/40">
          <div
            className={`h-full ${barTone}`}
            style={{ width: `${pct}%` }}
            aria-label={`confidence ${pct}%`}
          />
        </div>
        <span className="text-xs uppercase tracking-widest opacity-70">
          <T id="waybill.ai.confidence" />
        </span>
      </div>

      {rationale && (
        <p className="font-sans text-sm leading-snug opacity-90">{rationale}</p>
      )}

      <div className="font-mono text-xs uppercase tracking-widest opacity-60">
        model: {modelName} · {latencyMs}ms ·{' '}
        <T id="waybill.ai.neverAutoClicks" />
      </div>
    </div>
  );
}

export default AiRecommendChip;
