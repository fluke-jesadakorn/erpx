'use client';
import { useEffect, useRef, useState } from 'react';
import { T } from '@/components/i18n/T';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { deriveScope } from './scope';
import { useChatSession } from './useChatSession';
import { MessageRenderer } from './MessageRenderer';
import { SessionList } from './SessionList';

interface Props {
  initialSessions: { id: string; userId: number; title: string; modelName: string; createdAt: string; updatedAt: string }[];
  initialSessionId?: string;
}

export function FullChat({ initialSessions, initialSessionId }: Props) {
  const locale = useSecondaryLocale();
  const [input, setInput] = useState('');
  const scroller = useRef<HTMLDivElement>(null);

  const scope = deriveScope('/chat', '') ?? {
    tileId: 'global',
    displayName: 'Folio',
    hint: 'Full AI chat.',
    quickPrompts: [],
    sectionKey: 'chat:full',
  };
  const cs = useChatSession({ scope: { tileId: scope.tileId, displayName: scope.displayName, hint: scope.hint } });

  useEffect(() => {
    if (initialSessionId) {
      try { localStorage.setItem('folio.chat.global.sessionId', initialSessionId); } catch { /* ignore */ }
    }
  }, [initialSessionId]);

  useEffect(() => {
    if (scroller.current) scroller.current.scrollTop = scroller.current.scrollHeight;
  }, [cs.messages, cs.streamingContent, cs.pending]);

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const t = input.trim();
    if (!t) return;
    setInput('');
    void cs.send(t);
  };

  return (
    <div className="flex h-[calc(100vh-7rem)] gap-3">
      <aside className="w-64 shrink-0 overflow-y-auto rounded-md border border-rule bg-paper-2/60 p-2 text-sm">
        <SessionList
          sessions={cs.sessions.length > 0 ? cs.sessions : (initialSessions as any)}
          activeId={cs.sessionId}
          onSwitch={(id) => { void cs.switchSession(id); }}
          onDelete={(id) => { void cs.deleteSession(id); }}
          onNew={() => { void cs.newSession(); }}
          newLabel="New chat"
          sessionsLabel="Sessions"
        />
        <div className="mt-3 border-t border-rule pt-2 px-2 text-[10px] font-mono uppercase tracking-widest text-mute">
          {cs.model} · {cs.thinking}
        </div>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col rounded-md border border-rule bg-paper-2/60">
        <header className="flex items-center gap-3 border-b border-rule px-4 py-2 text-sm">
          <span className="font-mono text-accent">
            <T id="chat.global.titleWith" values={{ scope: scope.displayName }} />
          </span>
          <span className="text-xs text-mute">
            <T id="chat.global.subtitle" />
          </span>
          <div className="ml-auto flex items-center gap-2 text-xs">
            <label className="text-mute">
              <T id="chat.global.model" />
            </label>
            <input
              value={cs.model}
              onChange={(e) => cs.setModel(e.target.value)}
              className="w-32 rounded border border-rule bg-paper px-2 py-1 font-mono text-xs text-ink"
            />
            <select
              value={cs.thinking}
              onChange={(e) => cs.setThinking(e.target.value as 'low' | 'medium' | 'high')}
              className="rounded border border-rule bg-paper px-2 py-1 text-xs text-ink"
            >
              <option value="low">low</option>
              <option value="medium">medium</option>
              <option value="high">high</option>
            </select>
          </div>
        </header>

        <div ref={scroller} className="flex-1 space-y-2 overflow-y-auto px-4 py-3">
          {cs.messages.length === 0 && !cs.pending && (
            <div className="py-12 text-center text-xs text-mute font-mono">
              <T id="chat.global.empty" />
            </div>
          )}
          {cs.messages.map((m) => (
            <MessageRenderer
              key={m.id}
              role={m.role}
              content={m.content}
              charts={m.blocks?.charts as any}
              htmls={m.blocks?.htmls}
              sqls={m.blocks?.sqls}
              modelName={m.modelName}
              latencyMs={m.latencyMs}
            />
          ))}
          {cs.pending && (
            <MessageRenderer
              role="assistant"
              content={cs.streamingContent}
              charts={cs.streamingBlocks.charts as any}
              htmls={cs.streamingBlocks.htmls}
              sqls={cs.streamingBlocks.sqls}
              pending
            />
          )}
        </div>

        <div className="border-t border-rule px-3 py-2">
          <div className="mb-2 flex gap-2 overflow-x-auto">
            {scope.quickPrompts.map((p, i) => (
              <button
                key={i}
                type="button"
                disabled={cs.pending}
                onClick={() => void cs.send(p.prompt)}
                className="shrink-0 rounded-full bg-paper-2 px-3 py-1 text-xs text-ink hover:bg-paper-2 disabled:opacity-50"
                title={p.prompt}
              >
                <span aria-hidden className="mr-1">{p.icon}</span>
                {(p as any)[`label_${locale}`] ?? p.label}
              </button>
            ))}
          </div>
          <form onSubmit={onSubmit} className="flex gap-2">
            <input
              value={input}
              onChange={(e) => setInput(e.target.value)}
              placeholder="Ask anything…"
              disabled={cs.pending}
              className="flex-1 rounded-lg border border-rule bg-paper px-3 py-2 text-sm text-ink placeholder-mute focus:border-accent focus:outline-none disabled:opacity-50"
            />
            {cs.pending ? (
              <button
                type="button"
                onClick={cs.abort}
                className="rounded-lg bg-critical-strong px-4 text-sm text-ink hover:bg-critical"
              >
                <T id="chat.global.stop" />
              </button>
            ) : (
              <button
                type="submit"
                disabled={!input.trim()}
                className="rounded-lg bg-accent-strong px-4 text-sm font-medium text-ink hover:bg-accent disabled:opacity-50"
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