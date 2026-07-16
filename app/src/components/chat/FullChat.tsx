'use client';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '@/components/i18n/useT';
import { T } from '@/components/i18n/T';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import chatDict from '@folio-lib/i18n/chat';
import { DEFAULT_CHAT_MODEL } from '@folio-lib/ai/defaults';
import { streamChat } from '@/lib/chat/streaming';
import type { ChatSession, ChatMessage, ChatBlocks, SqlResolved } from '@/lib/chat/history';
import { MessageRenderer } from './MessageRenderer';
import type { ChartSpec } from '@/components/chat/chartContract';
import type { BilingualText } from '@folio-lib/i18n/types';

interface Props {
  initialSessions: ChatSession[];
  initialSessionId?: string;
}

interface StreamBlocks {
  charts: ChartSpec[];
  htmls: string[];
  sqls: SqlResolved[];
}

export function FullChat({ initialSessions, initialSessionId }: Props) {
  const t = useT(chatDict);
  const locale = useSecondaryLocale();
  const [sessions, setSessions] = useState(initialSessions);
  const [activeId, setActiveId] = useState<string | undefined>(initialSessionId);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [model, setModel] = useState(DEFAULT_CHAT_MODEL);
  const [thinking, setThinking] = useState<'low' | 'medium' | 'high'>('high');
  const [pending, setPending] = useState(false);
  const [streamingContent, setStreamingContent] = useState('');
  const [streamingBlocks, setStreamingBlocks] = useState<StreamBlocks>({
    charts: [],
    htmls: [],
    sqls: [],
  });
  const abortRef = useRef<AbortController | null>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const contentRef = useRef('');
  const appendContent = useCallback((delta: string) => {
    contentRef.current += delta;
    setStreamingContent(contentRef.current);
  }, []);
  const resetContent = useCallback(() => {
    contentRef.current = '';
    setStreamingContent('');
  }, []);

  const pick = (k: string): BilingualText => t(k);
  const text = (k: string) => pick(k)[locale] ?? pick(k).en;

  useEffect(() => {
    if (scroller.current) scroller.current.scrollTop = scroller.current.scrollHeight;
  }, [messages, streamingContent, pending]);

  useEffect(() => {
    if (!activeId) {
      setMessages([]);
      return;
    }
    let cancel = false;
    fetch(`/api/ai/chat/full/sessions/${activeId}`)
      .then((r) => r.json())
      .then((d) => {
        if (cancel) return;
        if (d?.messages) setMessages(d.messages as ChatMessage[]);
      })
      .catch(() => {});
    return () => {
      cancel = true;
    };
  }, [activeId]);

  const newChat = useCallback(async () => {
    const r = await fetch('/api/ai/chat/full/sessions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ model }),
    });
    const { session } = await r.json();
    if (session) {
      setSessions((s) => [session as ChatSession, ...s]);
      setActiveId(session.id);
      setMessages([]);
    }
  }, [model]);

  const deleteSession = useCallback(
    async (id: string) => {
      await fetch(`/api/ai/chat/full/sessions/${id}`, { method: 'DELETE' });
      setSessions((s) => s.filter((x) => x.id !== id));
      if (activeId === id) {
        setActiveId(undefined);
        setMessages([]);
      }
    },
    [activeId],
  );

  const send = useCallback(
    async (textArg?: string) => {
      const content = (textArg ?? input).trim();
      if (!content || pending) return;
      setInput('');
      const userMsg: ChatMessage = {
        id: `t${Date.now()}`,
        sessionId: activeId ?? '',
        role: 'user',
        content,
        blocks: { plain: content, charts: [], htmls: [], sqls: [] },
        createdAt: new Date().toISOString(),
      };
      setMessages((m) => [...m, userMsg]);
      setPending(true);
      resetContent();
      setStreamingBlocks({ charts: [], htmls: [], sqls: [] });

      const ctrl = new AbortController();
      abortRef.current = ctrl;
      try {
        const history = messages.map((m) => ({ role: m.role, content: m.content }));
        await streamChat(
          {
            messages: [...history, { role: 'user', content }],
            sessionId: activeId,
            model,
            thinking,
            lang: locale,
          },
          {
            signal: ctrl.signal,
            onChunk: appendContent,
            onMeta: (meta) => {
              if (meta.sessionId && meta.sessionId !== activeId) setActiveId(meta.sessionId);
              if (meta.blocks) setStreamingBlocks(meta.blocks as StreamBlocks);
              const aiMsg: ChatMessage = {
                id: `t${Date.now() + 1}`,
                sessionId: meta.sessionId ?? activeId ?? '',
                role: 'assistant',
                content: contentRef.current,
                blocks: (meta.blocks as unknown as ChatBlocks) ?? {
                  plain: contentRef.current,
                  charts: [],
                  htmls: [],
                  sqls: [],
                },
                modelName: meta.modelName ?? null,
                latencyMs: meta.latencyMs ?? null,
                createdAt: new Date().toISOString(),
              };
              setMessages((m) => [...m, aiMsg]);
              resetContent();
            },
            onError: (e) => {
              const errMsg: ChatMessage = {
                id: `t${Date.now()}`,
                sessionId: activeId ?? '',
                role: 'system',
                content: `⚠️ ${e.message}`,
                blocks: { plain: '', charts: [], htmls: [], sqls: [] },
                createdAt: new Date().toISOString(),
              };
              setMessages((m) => [...m, errMsg]);
            },
          },
        );
      } finally {
        setPending(false);
        abortRef.current = null;
      }
    },
    [input, pending, messages, activeId, model, thinking, locale, appendContent, resetContent],
  );

  const suggestions = [
    'chat.full.suggestion.0',
    'chat.full.suggestion.1',
    'chat.full.suggestion.2',
    'chat.full.suggestion.3',
  ];

  return (
    <div className="flex h-[calc(100vh-9rem)] gap-3">
      <aside className="w-64 shrink-0 overflow-y-auto rounded-2xl border border-slate-800 bg-slate-950/60 p-2 text-sm">
        <button
          type="button"
          onClick={newChat}
          className="mb-2 w-full rounded-lg bg-indigo-600 px-3 py-2 text-sm font-medium text-white hover:bg-indigo-500"
        >
          <T value={pick('chat.full.new')} />
        </button>
        <div className="mb-2 px-2 text-xs font-mono uppercase tracking-widest text-slate-500">
          <T value={pick('chat.full.sessions')} />
        </div>
        <ul className="space-y-1">
          {sessions.map((s) => (
            <li
              key={s.id}
              className={`group flex items-center gap-1 rounded-lg px-2 py-1.5 ${
                activeId === s.id
                  ? 'bg-indigo-500/15 text-indigo-200'
                  : 'text-slate-300 hover:bg-slate-900'
              }`}
            >
              <button
                type="button"
                onClick={() => setActiveId(s.id)}
                className="flex-1 truncate text-left"
              >
                {s.title}
              </button>
              <button
                type="button"
                onClick={() => deleteSession(s.id)}
                className="opacity-0 group-hover:opacity-100 text-slate-500 hover:text-rose-300"
              >
                ✕
              </button>
            </li>
          ))}
        </ul>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col rounded-2xl border border-slate-800 bg-slate-950/60">
        <header className="flex items-center gap-3 border-b border-slate-800 px-4 py-2 text-sm">
          <span className="font-mono text-indigo-300">
            🤖 <T value={pick('chat.full.title')} />
          </span>
          <span className="text-xs text-slate-500">
            <T value={pick('chat.full.subtitle')} />
          </span>
          <div className="ml-auto flex items-center gap-2 text-xs">
            <label className="text-slate-500">
              <T value={pick('chat.full.model')} />
            </label>
            <input
              value={model}
              onChange={(e) => setModel(e.target.value)}
              className="w-32 rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs text-slate-200"
            />
            <select
              value={thinking}
              onChange={(e) => setThinking(e.target.value as 'low' | 'medium' | 'high')}
              className="rounded border border-slate-700 bg-slate-900 px-2 py-1 text-xs text-slate-200"
            >
              <option value="low">
                <T value={pick('chat.full.thinkingLow')} />
              </option>
              <option value="medium">
                <T value={pick('chat.full.thinkingMed')} />
              </option>
              <option value="high">
                <T value={pick('chat.full.thinkingHigh')} />
              </option>
            </select>
          </div>
        </header>

        <div ref={scroller} className="flex-1 space-y-2 overflow-y-auto px-4 py-3">
          {messages.length === 0 && !pending && (
            <div className="py-12 text-center text-xs text-slate-500 font-mono">
              <T value={pick('chat.full.empty')} />
            </div>
          )}
          {messages.map((m) => (
            <MessageRenderer
              key={m.id}
              role={m.role}
              content={m.content}
              charts={m.blocks?.charts as ChartSpec[] | undefined}
              htmls={m.blocks?.htmls}
              sqls={m.blocks?.sqls}
              modelName={m.modelName}
              latencyMs={m.latencyMs}
            />
          ))}
          {pending && (
            <MessageRenderer
              role="assistant"
              content={streamingContent}
              charts={streamingBlocks.charts}
              htmls={streamingBlocks.htmls}
              sqls={streamingBlocks.sqls}
              pending
            />
          )}
        </div>

        <div className="border-t border-slate-800 px-3 py-2">
          <div className="mb-2 flex gap-2 overflow-x-auto">
            {suggestions.map((k, i) => {
              const label = pick(k);
              return (
                <button
                  key={i}
                  type="button"
                  disabled={pending}
                  onClick={() => send(text(k))}
                  className="shrink-0 rounded-full bg-slate-800 px-3 py-1 text-xs text-slate-200 hover:bg-slate-700 disabled:opacity-50"
                >
                  <T value={label} />
                </button>
              );
            })}
          </div>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              send();
            }}
            className="flex gap-2"
          >
            <input
              value={input}
              onChange={(e) => setInput(e.target.value)}
              placeholder={text('chat.full.inputPh')}
              disabled={pending}
              className="flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-white placeholder-slate-500 focus:border-indigo-500 focus:outline-none disabled:opacity-50"
            />
            {pending ? (
              <button
                type="button"
                onClick={() => abortRef.current?.abort()}
                className="rounded-lg bg-rose-600 px-4 text-sm text-white hover:bg-rose-500"
              >
                <T value={pick('chat.full.stop')} />
              </button>
            ) : (
              <button
                type="submit"
                disabled={!input.trim()}
                className="rounded-lg bg-indigo-600 px-4 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
              >
                ➤
              </button>
            )}
          </form>
        </div>
      </section>
    </div>
  );
}
