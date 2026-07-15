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
  indigo:  'bg-indigo-500/20 text-indigo-200 border-indigo-500/30 hover:bg-indigo-500/30',
  amber:   'bg-amber-500/20 text-amber-200 border-amber-500/30 hover:bg-amber-500/30',
  emerald: 'bg-emerald-500/20 text-emerald-200 border-emerald-500/30 hover:bg-emerald-500/30',
  cyan:    'bg-cyan-500/20 text-cyan-200 border-cyan-500/30 hover:bg-cyan-500/30',
  purple:  'bg-purple-500/20 text-purple-200 border-purple-500/30 hover:bg-purple-500/30',
  rose:    'bg-rose-500/20 text-rose-200 border-rose-500/30 hover:bg-rose-500/30',
};

const TONE_CARD: Record<NonNullable<AiActionButtonProps['tone']>, string> = {
  indigo:  'border-indigo-500/30 bg-indigo-950/20 text-indigo-100',
  amber:   'border-amber-500/30 bg-amber-950/20 text-amber-100',
  emerald: 'border-emerald-500/30 bg-emerald-950/20 text-emerald-100',
  cyan:    'border-cyan-500/30 bg-cyan-950/20 text-cyan-100',
  purple:  'border-purple-500/30 bg-purple-950/20 text-purple-100',
  rose:    'border-rose-500/30 bg-rose-950/20 text-rose-100',
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
        <div className={`rounded-2xl border p-3 ${TONE_CARD[tone]}`}>
          <div className="flex items-center justify-between gap-2 mb-1.5">
            <div className="text-xs font-mono font-bold uppercase tracking-wider opacity-80">
              {resultTitle || sectionKey}
              {meta?.modelName && (
                <span className="ml-2 px-1.5 py-0.5 rounded bg-black/30 text-xs font-mono normal-case">
                  {meta.modelName}{meta.latencyMs != null ? ` · ${meta.latencyMs}ms` : ''}
                </span>
              )}
            </div>
            <div className="flex items-center gap-1">
              {text && (
                <button
                  type="button"
                  onClick={copy}
                  className="text-xs px-2 py-0.5 rounded bg-black/30 hover:bg-black/50 font-mono"
                >
                  {copied ? '✓ Copied' : 'Copy'}
                </button>
              )}
              <button
                type="button"
                onClick={close}
                aria-label="Close result"
                title="Close"
                className="text-xs w-5 h-5 inline-flex items-center justify-center rounded bg-black/30 hover:bg-rose-900/60 font-mono"
              >
                ✕
              </button>
            </div>
          </div>
          {error ? (
            <p className="text-sm text-rose-300 font-mono">✗ {error}</p>
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

// MiniMax M3 wraps every reply in <think>...</think>. Strip that for the UI.
function stripThinking(s: string): string {
  if (!s) return s;
  return s.replace(/<think>[\s\S]*?<\/think>/g, '').trim();
}
