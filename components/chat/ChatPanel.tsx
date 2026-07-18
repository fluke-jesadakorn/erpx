'use client';
import { useEffect, useRef, useState } from 'react';
import { T } from '@/components/i18n/T';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import type { ChatScope } from './scope';
import { useChatSession } from './useChatSession';
import { MessageRenderer } from './MessageRenderer';
import { SessionList } from './SessionList';

export function ChatPanel({
  scope,
  onClose,
}: {
  scope: ChatScope;
  onClose: () => void;
}) {
  const locale = useSecondaryLocale();
  const [input, setInput] = useState('');
  const [showSessions, setShowSessions] = useState(false);
  const scroller = useRef<HTMLDivElement>(null);

  const cs = useChatSession({ scope: { tileId: scope.tileId, displayName: scope.displayName, hint: scope.hint } });

  useEffect(() => {
    if (scroller.current) scroller.current.scrollTop = scroller.current.scrollHeight;
  }, [cs.messages, cs.streamingContent, cs.pending]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const t = input.trim();
    if (!t) return;
    setInput('');
    void cs.send(t);
  };

  return (
    <>
      <div
        role="button"
        aria-label="backdrop"
        tabIndex={-1}
        onClick={onClose}
        className="fixed inset-0 z-sticky bg-paper/30"
      />
      <aside className="fixed right-0 top-0 bottom-0 z-fixed flex w-full max-w-md flex-col border-l border-rule bg-paper-2/95 backdrop-blur-md shadow-2xl text-ink">
        <header className="flex items-center gap-2 border-b border-rule px-3 py-2 text-sm">
          <span className="font-mono text-accent">
            <T id="chat.global.titleWith" values={{ scope: scope.displayName }} />
          </span>
          <span className="ml-auto" />
          <input
            value={cs.model}
            onChange={(e) => cs.setModel(e.target.value)}
            className="w-24 rounded border border-rule bg-paper px-2 py-1 font-mono text-xs text-ink"
            title="model"
            aria-label="model"
          />
          <select
            value={cs.thinking}
            onChange={(e) => cs.setThinking(e.target.value as 'low' | 'medium' | 'high')}
            className="rounded border border-rule bg-paper px-2 py-1 text-xs text-ink"
            aria-label="thinking"
          >
            <option value="low">low</option>
            <option value="medium">medium</option>
            <option value="high">high</option>
          </select>
          <button
            type="button"
            onClick={() => setShowSessions((s) => !s)}
            className="rounded border border-rule px-2 py-1 text-xs text-ink-2 hover:bg-paper-2"
            title="sessions"
          >
            ☰
          </button>
          <button
            type="button"
            onClick={onClose}
            aria-label="close"
            className="text-ink-2 hover:text-ink text-lg w-8 h-8 inline-flex items-center justify-center rounded-full hover:bg-paper-2"
          >
            ✕
          </button>
        </header>

        {showSessions ? (
          <div className="flex-1 overflow-y-auto p-3">
            <SessionList
              sessions={cs.sessions}
              activeId={cs.sessionId}
              onSwitch={(id) => { void cs.switchSession(id); setShowSessions(false); }}
              onDelete={(id) => { void cs.deleteSession(id); }}
              onNew={() => { void cs.newSession(); setShowSessions(false); }}
              newLabel="New chat"
              sessionsLabel="Sessions"
            />
          </div>
        ) : (
          <>
            <div className="border-b border-rule px-3 py-2">
              <div className="flex gap-2 overflow-x-auto whitespace-nowrap">
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
            </div>

            <div ref={scroller} className="flex-1 space-y-2 overflow-y-auto px-3 py-3">
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

            <form onSubmit={onSubmit} className="border-t border-rule px-3 py-2 flex gap-2">
              <input
                value={input}
                onChange={(e) => setInput(e.target.value)}
                placeholder={cs.sessionId ? '' : `Ask anything… (${scope.displayName})`}
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
          </>
        )}
      </aside>
    </>
  );
}