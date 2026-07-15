'use client';
import React, { useEffect, useRef, useState } from 'react';
import { ChartRenderer } from './ChartRenderer';
import { PinToCockpitButton } from './PinToCockpitButton';
import { parseChartBlocks, type ChartSpec } from './chartContract';
import { QUICK_PROMPTS } from './quickPrompts';
import { useT } from '@/components/i18n/useT';
import { T, interpolate } from '@/components/i18n/T';
import type { BilingualText } from '@folio-lib/i18n/types';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import chatDict from '@folio-lib/i18n/chat';

type Role = 'user' | 'assistant' | 'system';

interface ChatMessage {
  role: Role;
  content: string;
  modelName?: string;
  latencyMs?: number;
}

interface PerTileChatProps {
  tileId: string;
  sectionKey: string;
  displayName?: string;
  contextData?: any;
  lang?: 'en' | 'th' | 'de';
  expenseDraftId?: string;
}

function storageKey(tileId: string) {
  return `folio.chat.${tileId}.v1`;
}

function textOf(b: BilingualText, locale: 'th' | 'de'): string {
  return b[locale] ?? b.en;
}

function lineOf(b: BilingualText, locale: 'th' | 'de'): string {
  const s = b[locale];
  return s && s !== b.en ? `${b.en} · ${s}` : b.en;
}

export function PerTileChat({
  tileId,
  sectionKey,
  displayName,
  expenseDraftId,
}: PerTileChatProps) {
  const t = useT(chatDict);
  const locale = useSecondaryLocale();
  const [open, setOpen] = useState(false);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const pick = (k: string): BilingualText => t(k);
  const plain = (k: string, vars: Record<string, string | number> = {}) => textOf(interpolate(pick(k), vars), locale);
  const line = (k: string, vars: Record<string, string | number> = {}) => lineOf(interpolate(pick(k), vars), locale);

  const failedLabel = line('chat.error.ai');
  const networkLabel = line('chat.error.network');

  useEffect(() => {
    try {
      const raw = localStorage.getItem(storageKey(tileId));
      if (raw) {
        const parsed = JSON.parse(raw);
        if (Array.isArray(parsed)) setMessages(parsed);
      }
    } catch {}
  }, [tileId]);

  useEffect(() => {
    try {
      const trimmed = messages.slice(-200);
      localStorage.setItem(storageKey(tileId), JSON.stringify(trimmed));
    } catch {}
  }, [messages, tileId]);

  useEffect(() => {
    if (scroller.current) {
      scroller.current.scrollTop = scroller.current.scrollHeight;
    }
  }, [messages, open]);

  const prompts = QUICK_PROMPTS[tileId] ?? QUICK_PROMPTS['cockpit'];

  async function send(text: string) {
    const trimmed = text.trim();
    if (!trimmed || busy) return;
    setError(null);
    const userMsg: ChatMessage = { role: 'user', content: trimmed };
    const history = [...messages, userMsg].slice(-20);
    setMessages((m) => [...m, userMsg]);
    setInput('');
    setBusy(true);

    try {
      const r = await fetch('/api/ai/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          sectionKey,
          tileId,
          messages: history.map((m) => ({ role: m.role, content: m.content })),
          lang: locale,
        }),
      });
      const data = await r.json();
      if (!data.ok) {
        setError(data.error || failedLabel);
        const aiMsg: ChatMessage = {
          role: 'assistant',
          content: `🤖 ${line('chat.aiUnavailable', { error: data.error || 'unknown error' })}`,
        };
        setMessages((m) => [...m, aiMsg]);
      } else {
        const aiMsg: ChatMessage = {
          role: 'assistant',
          content: data.text || '',
          modelName: data.modelName,
          latencyMs: data.latencyMs,
        };
        setMessages((m) => [...m, aiMsg]);
      }
    } catch (e: any) {
      setError(e?.message || networkLabel);
      const aiMsg: ChatMessage = {
        role: 'assistant',
        content: `🤖 ${line('chat.aiUnavailable', { error: e?.message || networkLabel })}`,
      };
      setMessages((m) => [...m, aiMsg]);
    }
    setBusy(false);
  }

  function renderMessage(msg: ChatMessage, idx: number) {
    const isUser = msg.role === 'user';
    const bubbleCls = isUser
      ? 'bg-indigo-600/20 border-indigo-500/30'
      : 'bg-slate-900 border-slate-700';
    const align = isUser ? 'ml-auto items-end' : 'mr-auto items-start';

    if (isUser) {
      return (
        <div key={idx} className={`flex flex-col ${align} max-w-[85%]`}>
          <div className={`rounded-2xl px-3 py-2 border ${bubbleCls} text-sm text-white whitespace-pre-wrap break-words`}>
            {msg.content}
          </div>
        </div>
      );
    }

    const { plain, charts } = parseChartBlocks(msg.content);
    const extracts: never[] = [];

    return (
      <div key={idx} className={`flex flex-col ${align} max-w-[92%]`}>
        <div className={`rounded-2xl px-3 py-2 border ${bubbleCls} text-sm text-white break-words`}>
          {plain && <div className="whitespace-pre-wrap">{plain}</div>}
          {charts.map((c: ChartSpec, ci: number) => (
            <React.Fragment key={ci}>
              <ChartRenderer spec={c} />
              {ci === 0 && (
                <div className="mt-1">
                  <PinToCockpitButton spec={charts[0]} tileId={tileId} />
                </div>
              )}
            </React.Fragment>
          ))}
          {tileId === 'expense' && extracts.length > 0 && (
            <div className="mt-2 text-xs font-mono text-amber-300/80">
              (legacy extract fields — submit slip OCR instead)
            </div>
          )}
        </div>
        <div className="flex items-center gap-2 mt-1 px-1">
          <button
            type="button"
            onClick={() => navigator.clipboard?.writeText(msg.content)}
            className="text-xs font-mono uppercase tracking-wider text-slate-500 hover:text-slate-300"
          >
            <T value={pick('chat.copy')} />
          </button>
          {(msg.modelName || msg.latencyMs != null) && (
            <span className="text-xs font-mono text-slate-500">
              {msg.modelName ? msg.modelName : ''}
              {msg.latencyMs != null ? ` · ${msg.latencyMs}ms` : ''}
            </span>
          )}
        </div>
      </div>
    );
  }

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="fixed bottom-4 right-4 z-40 rounded-full bg-slate-900 border border-indigo-500/40 text-indigo-200 text-sm font-mono px-4 py-2 shadow-xl hover:bg-slate-800"
        title={plain('chat.open', { name: displayName || tileId })}
      >
        <T value={interpolate(pick('chat.label'), { tile: tileId })} />
      </button>

      {open && (
        <aside className="fixed right-0 top-0 bottom-0 w-full max-w-md z-50 bg-slate-950/95 backdrop-blur-md shadow-2xl border-l border-slate-800 overflow-y-auto flex flex-col text-slate-100">
          <header className="flex items-center justify-between px-4 py-3 border-b border-slate-800 sticky top-0 bg-slate-950/95 backdrop-blur-md z-10">
            <div className="text-sm font-mono">
              <T value={interpolate(pick('chat.header'), { tile: tileId })} />
              {displayName ? <span className="text-slate-500"> · {displayName}</span> : null}
            </div>
            <button
              type="button"
              onClick={() => setOpen(false)}
              className="text-slate-400 hover:text-white text-lg w-8 h-8 flex items-center justify-center rounded-full hover:bg-slate-800"
              aria-label={plain('chat.closeAria')}
            >
              ✕
            </button>
          </header>

          <div className="px-3 py-2 border-b border-slate-800 flex gap-2 overflow-x-auto whitespace-nowrap">
            {prompts.map((p, i) => (
              <button
                key={i}
                type="button"
                onClick={() => send(p.prompt)}
                disabled={busy}
                className="text-xs px-3 py-1.5 rounded-full bg-slate-800 text-slate-200 hover:bg-slate-700 disabled:opacity-50 shrink-0"
              >
                {p.icon} <T value={{ en: p.label, th: p.label_th, de: p.label_de }} />
              </button>
            ))}
          </div>

          <div ref={scroller} className="flex-1 overflow-y-auto px-3 py-3 space-y-2">
            {messages.length === 0 && (
              <div className="text-center text-xs text-slate-500 py-12 font-mono">
                <T value={interpolate(pick('chat.empty'), { tile: displayName || tileId })} />
              </div>
            )}
            {messages.map(renderMessage)}
            {busy && (
              <div className="flex justify-start">
                <div className="rounded-2xl px-3 py-2 border bg-rose-950/30 border-rose-700/50 text-xs text-rose-200 font-mono animate-pulse">
                  <T value={pick('chat.thinking')} />
                </div>
              </div>
            )}
            {error && (
              <div className="rounded-2xl px-3 py-2 border bg-rose-950/30 border-rose-700/50 text-xs text-rose-200 font-mono">
                🤖 <T value={interpolate(pick('chat.aiUnavailable'), { error: error ?? 'unknown' })} />
              </div>
            )}
          </div>

          <form
            onSubmit={(e) => {
              e.preventDefault();
              send(input);
            }}
            className="px-3 py-3 border-t border-slate-800 flex gap-2 bg-slate-950/95"
          >
            <input
              type="text"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              placeholder={plain('chat.inputPlaceholder')}
              disabled={busy}
              className="flex-1 rounded-lg bg-slate-900 border border-slate-700 px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:border-indigo-500 disabled:opacity-50"
            />
            <button
              type="submit"
              disabled={busy || !input.trim()}
              className="rounded-lg bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white text-sm font-medium px-4"
            >
              ➤
            </button>
          </form>
        </aside>
      )}
    </>
  );
}