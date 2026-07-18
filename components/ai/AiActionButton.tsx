'use client';

import React, { useState } from 'react';

type AITask = 'embed' | 'chat' | 'vision';

export interface AiActionButtonProps {
  sectionKey: string;
  task: AITask;
  systemPrompt?: string;
  input: string;
  buttonLabel: string;
  resultTitle?: string;
  tone?: 'indigo' | 'amber' | 'emerald' | 'cyan' | 'purple' | 'rose';
  size?: 'sm' | 'md';
  glyph?: string;
  resultPlaceholder?: string;
  onResult?: (text: string) => void;
}

const TONE_BG: Record<NonNullable<AiActionButtonProps['tone']>, string> = {
  indigo:  'bg-accent text-paper border-accent hover:bg-accent',
  amber:   'bg-caution-soft text-caution-strong border border-caution border-caution hover:bg-caution',
  emerald: 'bg-positive text-paper border-positive hover:bg-positive',
  cyan:    'bg-info text-paper border-info hover:bg-info',
  purple:  'bg-accent text-paper border-accent hover:bg-accent',
  rose:    'bg-critical-soft text-critical-strong border border-critical border-critical hover:bg-critical',
};

const TONE_CARD: Record<NonNullable<AiActionButtonProps['tone']>, string> = {
  indigo:  'border-accent bg-accent-strong text-accent-soft',
  amber:   'border-caution bg-caution-strong text-caution-soft',
  emerald: 'border-positive bg-positive-strong text-positive-soft',
  cyan:    'border-info bg-info-strong text-info-soft',
  purple:  'border-accent bg-accent-strong text-accent-soft',
  rose:    'border-critical bg-critical-strong text-critical-soft',
};

export const AiActionButton: React.FC<AiActionButtonProps> = ({
  sectionKey,
  task,
  systemPrompt,
  input,
  buttonLabel,
  resultTitle,
  tone = 'indigo',
  size = 'sm',
  glyph = '🤖',
  resultPlaceholder = 'AI result will appear here.',
  onResult,
}) => {
  const [busy, setBusy] = useState(false);
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [meta, setMeta] = useState<{ modelName?: string; latencyMs?: number } | null>(null);
  const [copied, setCopied] = useState(false);

  const sizeCls = size === 'sm'
    ? 'text-sm px-3 py-1.5'
    : 'text-xs px-4 py-2.5';

  async function run() {
    if (!input || !input.trim()) {
      setError('No input provided to AI');
      return;
    }
    setBusy(true);
    setError(null);
    setText(null);
    try {
      const res = await fetch('/api/ai/invoke', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          sectionKey,
          task,
          text: input,
          ...(systemPrompt ? { systemPrompt } : {}),
        }),
      });
      const data = await res.json();
      if (!data.ok) {
        setError(data.error || 'AI call failed');
      } else {
        setText(stripThinking(data.text || ''));
        setMeta({ modelName: data.modelName, latencyMs: data.latencyMs });
        onResult?.(data.text || '');
      }
    } catch (e: any) {
      setError(e?.message || 'Network error');
    } finally {
      setBusy(false);
    }
  }

  async function copy() {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {}
  }

  function close() {
    setText(null);
    setError(null);
    setMeta(null);
    setCopied(false);
  }

  return (
    <div className="space-y-2">
      <button
        type="button"
        onClick={run}
        disabled={busy}
        className={`inline-flex items-center gap-1.5 rounded-lg border font-bold transition-all ${sizeCls} ${TONE_BG[tone]} ${busy ? 'opacity-60 cursor-wait' : ''}`}
      >
        <span>{busy ? '⏳' : glyph}</span>
        <span>{busy ? 'Thinking…' : buttonLabel}</span>
      </button>

      {(text || error) && (
        <div className={`rounded-md border p-3 ${TONE_CARD[tone]}`}>
          <div className="flex items-center justify-between gap-2 mb-1.5">
            <div className="text-xs font-mono font-bold uppercase tracking-wider opacity-80">
              {resultTitle || sectionKey}
              {meta?.modelName && (
                <span className="ml-2 px-1.5 py-0.5 rounded bg-paper/30 text-xs font-mono normal-case">
                  {meta.modelName}{meta.latencyMs != null ? ` · ${meta.latencyMs}ms` : ''}
                </span>
              )}
            </div>
            <div className="flex items-center gap-1">
              {text && (
                <button
                  type="button"
                  onClick={copy}
                  className="text-xs px-2 py-0.5 rounded bg-paper/30 hover:bg-paper/50 font-mono"
                >
                  {copied ? '✓ Copied' : 'Copy'}
                </button>
              )}
              <button
                type="button"
                onClick={close}
                aria-label="Close result"
                title="Close"
                className="text-xs w-5 h-5 inline-flex items-center justify-center rounded bg-paper/30 hover:bg-critical-strong font-mono"
              >
                ✕
              </button>
            </div>
          </div>
          {error ? (
            <p className="text-sm text-critical font-mono">✗ {error}</p>
          ) : (
            <p className="text-xs whitespace-pre-wrap leading-relaxed font-sans">
              {text || resultPlaceholder}
            </p>
          )}
        </div>
      )}
    </div>
  );
};

function stripThinking(s: string): string {
  if (!s) return s;
  return s.replace(/<think>[\s\S]*?<\/think>/g, '').trim();
}