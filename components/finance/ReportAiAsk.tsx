'use client';

import { useState } from 'react';
import { Check, Copy, Send, Sparkles, X } from 'lucide-react';
import { MessageResponse } from '@/components/ai-elements/message';

interface ReportAiAskProps {
  title: string;
  scope: string;
  context: unknown;
  prompts: string[];
  canUseAi: boolean;
  className?: string;
}

const systemPrompt = `You are Folio Executive Intelligence, a rigorous CFO-grade finance analyst inside this company's ERP. Answer only from the supplied report context. Clearly distinguish posted actuals, operational pipeline, and simulated values. Never invent a figure or claim that is not supported by the context. Every numeric money value in the context is an exact raw THB amount: 158.41 means THB 158.41, never 158.41 thousand or million. Never use dollars or a $ symbol. If the data cannot answer the question, say exactly what is missing and suggest the closest ledger or report to inspect. Use concise executive markdown with a direct conclusion first, then evidence and risk or action.`;

export function ReportAiAsk({ title, scope, context, prompts, canUseAi, className = '' }: ReportAiAskProps) {
  const [open, setOpen] = useState(false);
  const [input, setInput] = useState('');
  const [answer, setAnswer] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [copied, setCopied] = useState(false);

  async function ask(value: string) {
    const question = value.trim();
    if (!question || busy || !canUseAi) return;
    setOpen(true);
    setBusy(true);
    setError('');
    setAnswer('');
    setInput('');
    setCopied(false);
    const raw = JSON.stringify(context);
    const clipped = raw.length > 24_000 ? `${raw.slice(0, 24_000)}\n[Context clipped at 24,000 characters]` : raw;
    try {
      const res = await fetch('/api/ai/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          sectionKey: 'chat:executive',
          tileId: 'executive',
          systemPrompt,
          messages: [{ role: 'user', content: `Report scope: ${scope}\nVerified context: ${clipped}\n\nExecutive question: ${question}` }],
        }),
      });
      const data = await res.json();
      if (!data?.ok) throw new Error(data?.error || 'AI analysis is unavailable');
      setAnswer(data.plain || data.text || 'No analysis returned.');
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'AI analysis is unavailable');
    } finally {
      setBusy(false);
    }
  }

  async function copy() {
    if (!answer) return;
    await navigator.clipboard.writeText(answer);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1_500);
  }

  return <div className={className}>
    <button
      type="button"
      onClick={() => setOpen((value) => !value)}
      disabled={!canUseAi}
      className="inline-flex h-9 items-center gap-2 rounded-full border border-accent/55 bg-accent-soft px-3.5 text-xs font-semibold text-accent-ink shadow-[0_10px_28px_-18px_var(--accent)] transition hover:-translate-y-0.5 hover:border-accent disabled:cursor-not-allowed disabled:opacity-45"
      title={canUseAi ? `Ask AI about ${scope}` : 'AI permission required'}
      aria-expanded={open}
    >
      <Sparkles size={14} aria-hidden />
      Ask Folio AI
    </button>
    {open && <div className="panel-floating mt-3 overflow-hidden border-accent/35">
      <div className="flex items-start justify-between gap-4 border-b border-rule px-4 py-3">
        <div>
          <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[0.14em] text-accent">
            <span className="relative flex h-2 w-2"><span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-accent opacity-50" /><span className="relative inline-flex h-2 w-2 rounded-full bg-accent" /></span>
            Executive intelligence
          </div>
          <h4 className="mt-1 text-sm font-semibold text-ink">{title}</h4>
          <p className="mt-0.5 text-xs text-mute">Scoped to {scope}</p>
        </div>
        <button type="button" onClick={() => setOpen(false)} className="rounded-full p-1.5 text-mute hover:bg-paper-3 hover:text-ink" aria-label="Close AI analysis"><X size={15} /></button>
      </div>
      <div className="space-y-3 p-4">
        <div className="flex flex-wrap gap-2">
          {prompts.map((prompt) => <button key={prompt} type="button" onClick={() => void ask(prompt)} disabled={busy} className="rounded-full border border-rule bg-paper-2 px-3 py-1.5 text-left text-[11px] text-ink-2 transition hover:border-accent hover:text-ink disabled:opacity-50">{prompt}</button>)}
        </div>
        {(busy || answer || error) && <div className="rounded-xl border border-rule bg-paper/70 p-4">
          {busy && <div className="flex items-center gap-2 text-sm text-ink-2"><Sparkles className="animate-pulse text-accent" size={15} /><span>Analyzing posted records and report context…</span></div>}
          {error && <p className="text-sm text-critical">{error}</p>}
          {answer && !busy && <div className="relative pr-8 text-sm leading-6 text-ink-2">
            <MessageResponse>{answer}</MessageResponse>
            <button type="button" onClick={() => void copy()} className="absolute right-0 top-0 rounded-md p-1.5 text-mute hover:bg-paper-3 hover:text-ink" aria-label="Copy AI analysis">{copied ? <Check className="text-positive" size={14} /> : <Copy size={14} />}</button>
          </div>}
        </div>}
        <form onSubmit={(event) => { event.preventDefault(); void ask(input); }} className="flex items-center gap-2 rounded-xl border border-rule bg-paper px-3 py-2 focus-within:border-accent">
          <input value={input} onChange={(event) => setInput(event.target.value)} className="min-w-0 flex-1 bg-transparent text-sm text-ink placeholder:text-mute" placeholder={`Ask a deeper question about ${scope}…`} aria-label={`Ask about ${scope}`} />
          <button type="submit" disabled={!input.trim() || busy} className="grid h-8 w-8 place-items-center rounded-lg bg-action text-action-ink transition hover:bg-action-hover disabled:opacity-35" aria-label="Send question"><Send size={14} /></button>
        </form>
        <p className="text-[10px] leading-4 text-mute">AI analysis is decision support. Posted ledgers remain the source of record.</p>
      </div>
    </div>}
  </div>;
}
