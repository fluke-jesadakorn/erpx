'use client';
import React, { useEffect, useRef, useState } from 'react';
import { ChartRenderer } from './ChartRenderer';
import { PinToCockpitButton } from './PinToCockpitButton';
import { parseChartBlocks, type ChartSpec } from './chartContract';
import { QUICK_PROMPTS } from './quickPrompts';
import { T } from '@/components/i18n/T';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';

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

export function PerTileChat({
  tileId,
  sectionKey,
  displayName,
  expenseDraftId: _expenseDraftId,
}: PerTileChatProps) {
  const locale = useSecondaryLocale();
  const [open, setOpen] = useState(false);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const scroller = useRef<HTMLDivElement>(null);

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
        setError(data.error || 'AI call failed');
        const aiMsg: ChatMessage = {
          role: 'assistant',
          content: `🤖 AI unavailable: ${data.error || 'unknown error'}`,
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
      setError(e?.message || 'Network error');
      const aiMsg: ChatMessage = {
        role: 'assistant',
        content: `🤖 AI unavailable: ${e?.message || 'network error'}`,
      };
      setMessages((m) => [...m, aiMsg]);
    }
    setBusy(false);
  }

  function renderMessage(msg: ChatMessage, idx: number) {
    const isUser = msg.role === 'user';
    const bubbleCls = isUser
      ? 'bg-accent-strong border-accent'
      : 'bg-paper border-rule';
    const align = isUser ? 'ml-auto items-end' : 'mr-auto items-start';

    if (isUser) {
      return (
        <div key={idx} className={`flex flex-col ${align} max-w-[85%]`}>
          <div className={`rounded-md px-3 py-2 border ${bubbleCls} text-sm text-ink whitespace-pre-wrap break-words`}>
            {msg.content}
          </div>
        </div>
      );
    }

    const { plain, charts } = parseChartBlocks(msg.content);
    const extracts: never[] = [];

    return (
      <div key={idx} className={`flex flex-col ${align} max-w-[92%]`}>
        <div className={`rounded-md px-3 py-2 border ${bubbleCls} text-sm text-ink break-words`}>
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
            <div className="mt-2 text-xs font-mono text-caution">
              (legacy extract fields — submit slip OCR instead)
            </div>
          )}
        </div>
        <div className="flex items-center gap-2 mt-1 px-1">
          <button
            type="button"
            onClick={() => navigator.clipboard?.writeText(msg.content)}
            className="text-xs font-mono uppercase tracking-wider text-mute hover:text-ink-2"
          >
            <T id="chat.copy" />
          </button>
          {(msg.modelName || msg.latencyMs != null) && (
            <span className="text-xs font-mono text-mute">
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
        className="fixed bottom-4 right-4 z-sticky rounded-full bg-paper border border-accent text-accent-ink text-sm font-mono px-4 py-2 shadow-xl hover:bg-paper-2"
        title={`Open AI chat for ${displayName || tileId}`}
      >
        <T id="chat.label" values={{ tile: tileId }} />
      </button>

      {open && (
        <aside className="fixed right-0 top-0 bottom-0 w-full max-w-md z-fixed bg-paper-2/95 backdrop-blur-md shadow-2xl border-l border-rule overflow-y-auto flex flex-col text-ink">
          <header className="flex items-center justify-between px-4 py-3 border-b border-rule sticky top-0 bg-paper-2/95 backdrop-blur-md z-10">
            <div className="text-sm font-mono">
              <T id="chat.header" values={{ tile: tileId }} />
              {displayName ? <span className="text-mute"> · {displayName}</span> : null}
            </div>
            <button
              type="button"
              onClick={() => setOpen(false)}
              className="text-ink-2 hover:text-ink text-lg w-8 h-8 flex items-center justify-center rounded-full hover:bg-paper-2"
              aria-label="close"
            >
              ✕
            </button>
          </header>

          <div className="px-3 py-2 border-b border-rule flex gap-2 overflow-x-auto whitespace-nowrap">
            {prompts.map((p, i) => (
              <button
                key={i}
                type="button"
                onClick={() => send(p.prompt)}
                disabled={busy}
                className="text-xs px-3 py-1.5 rounded-full bg-paper-2 text-ink hover:bg-paper-2 disabled:opacity-50 shrink-0"
              >
                {p.icon} {(p as any)[`label_${locale}`] ?? p.label}
              </button>
            ))}
          </div>

          <div ref={scroller} className="flex-1 overflow-y-auto px-3 py-3 space-y-2">
            {messages.length === 0 && (
              <div className="text-center text-xs text-mute py-12 font-mono">
                <T id="chat.empty" values={{ tile: displayName || tileId }} />
              </div>
            )}
            {messages.map(renderMessage)}
            {busy && (
              <div className="flex justify-start">
                <div className="rounded-md px-3 py-2 border bg-critical-strong border-critical-strong text-xs text-paper font-mono animate-pulse">
                  <T id="chat.thinking" />
                </div>
              </div>
            )}
            {error && (
              <div className="rounded-md px-3 py-2 border bg-critical-strong border-critical-strong text-xs text-paper font-mono">
                🤖 <T id="chat.aiUnavailable" values={{ error: error ?? 'unknown' }} />
              </div>
            )}
          </div>

          <form
            onSubmit={(e) => {
              e.preventDefault();
              send(input);
            }}
            className="px-3 py-3 border-t border-rule flex gap-2 bg-paper-2/95"
          >
            <input
              type="text"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              placeholder="Ask anything…"
              disabled={busy}
              className="flex-1 rounded-lg bg-paper border border-rule px-3 py-2 text-sm text-ink placeholder-mute focus:outline-none focus:border-accent disabled:opacity-50"
            />
            <button
              type="submit"
              disabled={busy || !input.trim()}
              className="action-button rounded-lg bg-action hover:bg-action-hover disabled:opacity-50 text-action-ink text-sm font-medium px-4"
            >
              ➤
            </button>
          </form>
        </aside>
      )}
    </>
  );
}
