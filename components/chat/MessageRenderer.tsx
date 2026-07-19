'use client';

import { useEffect, useState } from 'react';
import { Check, Copy, Pencil, Sparkles, X } from 'lucide-react';
import { MessageResponse } from '@/components/ai-elements/message';
import { ChartRenderer } from './ChartRenderer';
import { HtmlRenderer } from './HtmlRenderer';
import { SqlResultTable } from './SqlResultTable';
import type { ChartSpec } from '@/components/chat/chartContract';
import type { SqlResolved } from '@/chat/history';
import { T } from '@/components/i18n/T';

export interface MessageRenderInput {
  messageId?: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  charts?: ChartSpec[];
  htmls?: string[];
  sqls?: SqlResolved[];
  modelName?: string | null;
  latencyMs?: number | null;
  pending?: boolean;
  editDisabled?: boolean;
  onEdit?: (messageId: string, content: string) => void;
}

export function MessageRenderer({
  messageId,
  role,
  content,
  charts = [],
  htmls = [],
  sqls = [],
  modelName,
  latencyMs,
  pending,
  editDisabled,
  onEdit,
}: MessageRenderInput) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(content);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!editing) setDraft(content);
  }, [content, editing]);

  if (role === 'user') {
    const save = () => {
      const next = draft.trim();
      if (!messageId || !next || next === content) {
        setEditing(false);
        setDraft(content);
        return;
      }
      setEditing(false);
      onEdit?.(messageId, next);
    };

    return (
      <div className="group ml-auto w-fit max-w-[88%] sm:max-w-[78%]">
        <div className="rounded-2xl rounded-br-md border border-accent/35 bg-accent-strong px-4 py-3 text-sm text-ink shadow-panel">
          {editing ? (
            <div className="min-w-[min(32rem,70vw)]">
              <textarea
                autoFocus
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Escape') { setEditing(false); setDraft(content); }
                  if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); save(); }
                }}
                rows={Math.min(8, Math.max(2, draft.split('\n').length))}
                aria-label="Edit message"
                className="w-full resize-y rounded-xl border border-accent/40 bg-paper/95 px-3 py-2 text-sm text-ink outline-none focus:border-accent"
              />
              <div className="mt-2 flex justify-end gap-2">
                <button type="button" onClick={() => { setEditing(false); setDraft(content); }} className="inline-flex items-center gap-1 rounded-lg px-2 py-1 text-xs text-ink-2 hover:bg-paper/70"><X size={13} />Cancel</button>
                <button type="button" onClick={save} disabled={!draft.trim()} className="action-button inline-flex items-center gap-1 rounded-lg bg-action px-3 py-1 text-xs font-medium text-action-ink disabled:opacity-50"><Check size={13} />Save & rerun</button>
              </div>
            </div>
          ) : (
            <div className="whitespace-pre-wrap break-words">{content}</div>
          )}
        </div>
        {!editing && onEdit && messageId && (
          <div className="mt-1 flex justify-end opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
            <button type="button" disabled={editDisabled} onClick={() => setEditing(true)} className="inline-flex items-center gap-1 rounded-md px-2 py-1 font-mono text-[10px] text-mute hover:bg-paper-2 hover:text-ink disabled:opacity-40" aria-label="Edit message from this checkpoint">
              <Pencil size={11} /> edit checkpoint
            </button>
          </div>
        )}
      </div>
    );
  }

  if (pending) {
    return (
      <div className="mr-auto flex max-w-[92%] items-center gap-3 rounded-2xl border border-rule bg-paper/85 px-4 py-3 shadow-panel">
        <span className="relative flex size-8 items-center justify-center rounded-xl border border-accent/30 bg-accent/10 text-accent">
          <Sparkles size={15} className="animate-pulse" />
          <span className="absolute -inset-px animate-ping rounded-xl border border-accent/25" />
        </span>
        <div>
          <div className="text-xs font-medium text-ink"><T id="chat.thinking" /></div>
          <div className="mt-0.5 font-mono text-[10px] text-mute">querying · composing · rendering</div>
        </div>
      </div>
    );
  }

  const copy = () => {
    void navigator.clipboard.writeText(content).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    });
  };

  return (
    <div className={`group mr-auto w-full max-w-[96%] ${role === 'system' ? 'rounded-xl border border-critical/35 bg-critical/10 p-3' : ''}`}>
      {content && (
        <div className="mb-2 rounded-2xl rounded-tl-md border border-rule bg-paper/88 px-4 py-3 text-sm text-ink shadow-panel">
          <MessageResponse>{content}</MessageResponse>
        </div>
      )}
      {charts.map((chart, i) => <ChartRenderer key={`c${i}`} spec={chart} />)}
      {htmls.map((html, i) => <HtmlRenderer key={`h${i}`} html={html} />)}
      {sqls.map((sql, i) => <SqlResultTable key={`s${i}`} {...sql} />)}
      {(content || modelName || latencyMs != null) && (
        <div className="mt-1 flex items-center gap-2 px-1 font-mono text-[10px] text-mute">
          {modelName && <span>{modelName}</span>}
          {latencyMs != null && <span>· {(latencyMs / 1000).toFixed(1)}s</span>}
          {content && (
            <button type="button" onClick={copy} className="ml-auto inline-flex items-center gap-1 rounded px-1.5 py-1 opacity-0 transition-opacity hover:bg-paper-2 hover:text-ink group-hover:opacity-100 group-focus-within:opacity-100">
              {copied ? <Check size={11} /> : <Copy size={11} />}{copied ? 'copied' : 'copy'}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
