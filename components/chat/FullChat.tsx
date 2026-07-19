'use client';

import { useEffect, useRef, useState } from 'react';
import { Braces, Database, Menu, Send, Sparkles, Square, X } from 'lucide-react';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { AiModelControl } from '@/components/ai/AiModelControl';
import type { ChartSpec } from './chartContract';
import { MessageRenderer } from './MessageRenderer';
import { QUICK_PROMPTS } from './quickPrompts';
import { SessionList } from './SessionList';
import { useChatSession } from './useChatSession';

interface Props {
  initialSessions: { id: string; userId: number; title: string; modelName: string; createdAt: string; updatedAt: string }[];
  initialSessionId?: string;
}

const COPY = {
  en: { title: 'Folio Intelligence', subtitle: 'Ask across the entire operating system', placeholder: 'Ask about live data, workflows, people, finance, law, inventory…', empty: 'One conversation. Every part of Folio.', detail: 'Query live records, explore how the system works, compare scenarios, or run raw SELECT statements. Every result becomes an interactive artifact.', newChat: 'New analysis', sessions: 'History' },
  th: { title: 'Folio Intelligence', subtitle: 'ถามได้ทุกเรื่องภายในระบบ', placeholder: 'ถามเกี่ยวกับข้อมูล ขั้นตอน บุคลากร การเงิน กฎหมาย สินค้าคงคลัง…', empty: 'บทสนทนาเดียว เข้าถึงทุกส่วนของ Folio', detail: 'ค้นข้อมูลจริง ทำความเข้าใจขั้นตอน เปรียบเทียบสถานการณ์ หรือรันคำสั่ง SELECT โดยตรง ทุกคำตอบจะแสดงเป็น Interactive Artifact', newChat: 'วิเคราะห์ใหม่', sessions: 'ประวัติ' },
  de: { title: 'Folio Intelligence', subtitle: 'Fragen zum gesamten Betriebssystem', placeholder: 'Fragen zu Live-Daten, Abläufen, Personen, Finanzen, Recht, Inventar…', empty: 'Ein Gespräch. Jeder Bereich von Folio.', detail: 'Live-Daten abfragen, Abläufe verstehen, Szenarien vergleichen oder rohe SELECT-Abfragen ausführen. Ergebnisse erscheinen als interaktive Artefakte.', newChat: 'Neue Analyse', sessions: 'Verlauf' },
} as const;

const CHAT_SCOPE = { tileId: 'global', displayName: 'Folio', hint: 'Full-system intelligence workspace.' } as const;

export function FullChat({ initialSessions, initialSessionId }: Props) {
  const locale = useSecondaryLocale();
  const t = COPY[locale];
  const [input, setInput] = useState('');
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const scroller = useRef<HTMLDivElement>(null);
  const composer = useRef<HTMLTextAreaElement>(null);
  const prompts = QUICK_PROMPTS.chat ?? [];
  const cs = useChatSession({
    sectionKey: 'chat:global',
    scope: CHAT_SCOPE,
  });
  const switchSession = cs.switchSession;

  useEffect(() => {
    if (initialSessionId) void switchSession(initialSessionId);
  }, [initialSessionId, switchSession]);

  useEffect(() => {
    const node = scroller.current;
    if (node) node.scrollTo({ top: node.scrollHeight, behavior: cs.pending ? 'smooth' : 'auto' });
  }, [cs.messages, cs.streamingContent, cs.pending]);

  const submit = () => {
    const value = input.trim();
    if (!value || cs.pending) return;
    setInput('');
    void cs.send(value);
  };

  const sessions = cs.sessions.length > 0 ? cs.sessions : initialSessions;

  return (
    <div className="relative flex min-h-[680px] flex-1 overflow-hidden rounded-[24px] border border-rule bg-paper shadow-elevated lg:min-h-0">
      <div aria-hidden className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_85%_-10%,color-mix(in_oklab,var(--accent)_14%,transparent),transparent_34%)]" />

      {sidebarOpen && <button type="button" aria-label="Close history" onClick={() => setSidebarOpen(false)} className="absolute inset-0 z-20 bg-black/55 lg:hidden" />}
      <aside className={`absolute inset-y-0 left-0 z-30 flex w-[286px] flex-col border-r border-rule bg-paper-2/95 p-3 backdrop-blur-xl transition-transform lg:static lg:z-auto lg:translate-x-0 ${sidebarOpen ? 'translate-x-0' : '-translate-x-full'}`}>
        <div className="mb-3 flex items-center gap-2 px-1 pt-1">
          <div className="flex size-9 items-center justify-center rounded-xl border border-accent/35 bg-accent/10 text-accent"><Sparkles size={17} /></div>
          <div className="min-w-0">
            <div className="truncate text-sm font-semibold tracking-tight text-ink">{t.title}</div>
            <div className="font-mono text-[9px] uppercase tracking-[0.14em] text-mute">system workspace</div>
          </div>
          <button type="button" onClick={() => setSidebarOpen(false)} className="ml-auto rounded-lg p-1.5 text-mute hover:bg-paper hover:text-ink lg:hidden"><X size={16} /></button>
        </div>
        <SessionList
          sessions={sessions}
          activeId={cs.sessionId}
          onSwitch={(id) => { void cs.switchSession(id); setSidebarOpen(false); }}
          onDelete={(id) => { void cs.deleteSession(id); }}
          onNew={() => { void cs.newSession(); setSidebarOpen(false); composer.current?.focus(); }}
          newLabel={t.newChat}
          sessionsLabel={t.sessions}
        />
        <div className="mt-3 rounded-xl border border-rule bg-paper/70 p-3">
          <div className="flex items-center gap-2 font-mono text-[9px] uppercase tracking-[0.13em] text-mute"><span className="size-1.5 rounded-full bg-positive shadow-[0_0_10px_var(--positive)]" />live system access</div>
          <div className="mt-2 flex items-center gap-2 text-[10px] text-ink-2"><Database size={12} className="text-accent" />read-only PostgreSQL</div>
          <div className="mt-1 flex items-center gap-2 text-[10px] text-ink-2"><Braces size={12} className="text-accent" />HTML · CSS · JS artifacts</div>
        </div>
      </aside>

      <section className="relative flex min-w-0 flex-1 flex-col">
        <header className="flex min-h-16 flex-wrap items-center gap-3 border-b border-rule bg-paper/75 px-3 py-2.5 backdrop-blur-xl sm:px-5">
          <button type="button" onClick={() => setSidebarOpen(true)} className="rounded-xl border border-rule bg-paper-2 p-2 text-mute hover:text-ink lg:hidden"><Menu size={17} /></button>
          <div className="min-w-0">
            <h2 className="truncate text-sm font-semibold text-ink">{t.title}</h2>
            <p className="truncate text-[11px] text-mute">{t.subtitle}</p>
          </div>
          <div className="ml-auto flex w-full min-w-0 items-center gap-2 sm:w-auto">
            <span className="hidden items-center gap-1.5 rounded-full border border-positive/25 bg-positive/10 px-2 py-1 font-mono text-[9px] uppercase tracking-[0.11em] text-positive sm:inline-flex"><span className="size-1.5 rounded-full bg-positive" />live</span>
            <AiModelControl className="min-w-0 flex-1 sm:flex-none [&>select:first-child]:min-w-0 [&>select:first-child]:flex-1" sectionKey="chat:global" modelName={cs.model} thinkLevel={cs.thinking} onChange={(next) => cs.setModel(next.name)} onThinkChange={cs.setThinking} />
          </div>
        </header>

        <div ref={scroller} className="min-h-0 flex-1 space-y-5 overflow-y-auto px-3 py-5 sm:px-6">
          {cs.messages.length === 0 && !cs.pending && (
            <div className="mx-auto flex min-h-full max-w-4xl flex-col justify-center py-8">
              <div className="mb-8 max-w-2xl">
                <div className="mb-4 inline-flex size-12 items-center justify-center rounded-2xl border border-accent/35 bg-accent/10 text-accent shadow-[0_0_40px_color-mix(in_oklab,var(--accent)_18%,transparent)]"><Sparkles size={22} /></div>
                <h3 className="text-balance text-3xl font-semibold leading-tight tracking-[-0.045em] text-ink sm:text-5xl">{t.empty}</h3>
                <p className="mt-4 max-w-xl text-sm leading-relaxed text-ink-2">{t.detail}</p>
              </div>
              <div className="grid gap-2 sm:grid-cols-2">
                {prompts.map((prompt) => (
                  <button key={prompt.label} type="button" onClick={() => void cs.send(prompt.prompt)} className="group rounded-2xl border border-rule bg-paper-2/70 p-4 text-left transition hover:-translate-y-0.5 hover:border-accent/45 hover:bg-paper-3 hover:shadow-panel">
                    <span className="font-mono text-sm text-accent">{prompt.icon}</span>
                    <div className="mt-3 text-sm font-medium text-ink">{locale === 'th' ? prompt.label_th : locale === 'de' ? prompt.label_de : prompt.label}</div>
                    <div className="mt-1 line-clamp-2 text-xs leading-relaxed text-mute">{prompt.prompt}</div>
                  </button>
                ))}
              </div>
            </div>
          )}

          {cs.messages.map((message) => (
            <MessageRenderer
              key={message.id}
              messageId={message.id}
              role={message.role}
              content={message.role === 'assistant' ? (message.blocks?.plain || (message.blocks?.htmls?.length || message.blocks?.sqls?.length || message.blocks?.charts?.length ? '' : message.content)) : message.content}
              charts={message.blocks?.charts as ChartSpec[]}
              htmls={message.blocks?.htmls}
              sqls={message.blocks?.sqls}
              modelName={message.modelName}
              latencyMs={message.latencyMs}
              editDisabled={cs.pending}
              onEdit={(id, value) => void cs.editMessage(id, value)}
            />
          ))}
          {cs.pending && <MessageRenderer role="assistant" content={cs.streamingContent} pending />}
        </div>

        <footer className="border-t border-rule bg-paper/80 px-3 py-3 backdrop-blur-xl sm:px-5">
          {cs.messages.length > 0 && (
            <div className="mb-2 flex gap-2 overflow-x-auto pb-1">
              {prompts.slice(0, 3).map((prompt) => (
                <button key={prompt.label} type="button" disabled={cs.pending} onClick={() => void cs.send(prompt.prompt)} className="shrink-0 rounded-full border border-rule bg-paper-2 px-3 py-1.5 text-[11px] text-ink-2 hover:border-accent/40 hover:text-ink disabled:opacity-40">{prompt.icon} {locale === 'th' ? prompt.label_th : locale === 'de' ? prompt.label_de : prompt.label}</button>
              ))}
            </div>
          )}
          <div className="rounded-2xl border border-rule bg-paper-2/80 p-2 shadow-panel transition focus-within:border-accent/45 focus-within:ring-4 focus-within:ring-accent/10">
            <textarea
              ref={composer}
              value={input}
              onChange={(event) => setInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); submit(); }
              }}
              placeholder={t.placeholder}
              aria-label={t.placeholder}
              disabled={cs.pending}
              rows={2}
              className="block max-h-44 min-h-12 w-full resize-none bg-transparent px-2 py-1 text-sm text-ink outline-none placeholder:text-mute disabled:opacity-50"
            />
            <div className="flex items-center gap-2 px-1 pt-1">
              <span className="font-mono text-[9px] text-mute">ENTER to send · SHIFT+ENTER for line break</span>
              {cs.pending ? (
                <button type="button" onClick={cs.abort} className="ml-auto inline-flex size-9 items-center justify-center rounded-xl bg-critical text-paper hover:bg-critical-strong" aria-label="Stop response"><Square size={13} fill="currentColor" /></button>
              ) : (
                <button type="button" onClick={submit} disabled={!input.trim()} className="action-button ml-auto inline-flex size-9 items-center justify-center rounded-xl bg-action text-action-ink shadow-panel transition hover:-translate-y-0.5 hover:bg-action-hover disabled:translate-y-0 disabled:opacity-35" aria-label="Send message"><Send size={15} /></button>
              )}
            </div>
          </div>
        </footer>
      </section>
    </div>
  );
}
