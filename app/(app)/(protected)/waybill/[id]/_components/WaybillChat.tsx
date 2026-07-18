'use client';

import { useEffect, useRef, useState } from 'react';
import { LoaderCircle, Mail, Send } from 'lucide-react';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { Panel, Badge } from '@/components/ui';

interface Props {
  waybillId: string;
}

interface ChatMsg {
  id: string;
  role: 'user' | 'assistant';
  text: string;
  at: string;
}

export function WaybillChat({ waybillId }: Props) {
  const locale = useSecondaryLocale();
  const [messages, setMessages] = useState<ChatMsg[]>([]);
  const [input, setInput] = useState('');
  const [busy, setBusy] = useState(false);
  const scroller = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages]);

  async function send() {
    const trimmed = input.trim();
    if (!trimmed || busy) return;
    const userMsg: ChatMsg = { id: `u-${Date.now()}`, role: 'user', text: trimmed, at: new Date().toISOString() };
    setMessages((prev) => [...prev, userMsg]);
    setInput('');
    setBusy(true);
    try {
      const res = await fetch('/api/ai/chat', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          scope: 'tile',
          tileId: `waybill:${waybillId}`,
          displayName: `Waybill ${waybillId}`,
          message: trimmed,
          locale,
        }),
      });
      const json = await res.json();
      const text = json?.reply ?? json?.text ?? '…';
      setMessages((prev) => [...prev, { id: `a-${Date.now()}`, role: 'assistant', text, at: new Date().toISOString() }]);
    } catch (e: any) {
      setMessages((prev) => [...prev, { id: `e-${Date.now()}`, role: 'assistant', text: `Error: ${e?.message ?? String(e)}`, at: new Date().toISOString() }]);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Panel padding="none" className="flex h-[60vh] flex-col overflow-hidden">
      <header className="border-b border-rule px-5 py-3">
        <div className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-mute">
          <Mail size={12} aria-hidden />
          <span>Thread</span>
          <Badge tone="neutral" size="sm">{messages.length}</Badge>
          <span className="ml-auto">scope: <span className="text-accent">waybill:{waybillId}</span></span>
        </div>
      </header>
      <div ref={scroller} className="flex-1 space-y-2 overflow-y-auto px-5 py-4">
        {messages.length === 0 && (
          <p className="text-sm text-mute italic">
            Ask anything about this waybill · the assistant has the waybill context.
          </p>
        )}
        {messages.map((m) => (
          <div
            key={m.id}
            className={[
              'rounded-md border px-3 py-2 text-sm',
              m.role === 'user'
                ? 'ml-12 border-accent/40 bg-accent-soft text-ink'
                : 'mr-12 border-rule bg-paper-2 text-ink',
            ].join(' ')}
          >
            <div className="mb-1 text-[10px] font-mono uppercase tracking-widest text-mute">
              {m.role === 'user' ? 'you' : 'assistant'} · {new Date(m.at).toLocaleTimeString()}
            </div>
            <div className="whitespace-pre-wrap">{m.text}</div>
          </div>
        ))}
      </div>
      <footer className="border-t border-rule p-3">
        <div className="flex items-end gap-2">
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                void send();
              }
            }}
            rows={2}
            placeholder="Ask the assistant…"
            className="flex-1 rounded-lg border border-rule bg-paper px-3 py-2 text-sm text-ink placeholder:text-mute focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/30"
          />
          <button
            type="button"
            onClick={send}
            disabled={busy || !input.trim()}
            className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-4 py-2 text-sm font-bold text-paper-2 border border-accent hover:bg-accent-strong disabled:opacity-50 transition-colors"
          >
            {busy ? <LoaderCircle size={12} className="animate-spin" aria-hidden /> : <Send size={12} aria-hidden />}
            Send
          </button>
        </div>
      </footer>
    </Panel>
  );
}