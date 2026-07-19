'use client';

import { useMemo, useState } from 'react';
import { MessageSquareText, Plus, Search, Trash2 } from 'lucide-react';
import type { ChatSession } from '@/chat/history';

export function SessionList({
  sessions,
  activeId,
  onSwitch,
  onDelete,
  onNew,
  newLabel,
  sessionsLabel,
}: {
  sessions: ChatSession[];
  activeId: string | null;
  onSwitch: (id: string) => void;
  onDelete: (id: string) => void;
  onNew: () => void;
  newLabel: string;
  sessionsLabel: string;
}) {
  const [query, setQuery] = useState('');
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q ? sessions.filter((session) => session.title.toLowerCase().includes(q)) : sessions;
  }, [query, sessions]);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 text-sm">
      <button type="button" onClick={onNew} className="action-button inline-flex w-full items-center justify-center gap-2 rounded-xl bg-action px-3 py-2.5 text-sm font-medium text-action-ink shadow-panel transition hover:-translate-y-0.5 hover:bg-action-hover">
        <Plus size={15} />{newLabel}
      </button>
      <label className="relative block">
        <Search size={13} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-mute" />
        <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={sessionsLabel} aria-label={sessionsLabel} className="h-9 w-full rounded-xl border border-rule bg-paper pl-9 pr-3 text-xs text-ink outline-none placeholder:text-mute focus:border-accent/50" />
      </label>
      <div className="flex items-center justify-between px-1 font-mono text-[9px] uppercase tracking-[0.14em] text-mute">
        <span>{sessionsLabel}</span><span>{filtered.length}</span>
      </div>
      <ul className="min-h-0 flex-1 space-y-1 overflow-y-auto pr-1">
        {filtered.length === 0 ? (
          <li className="rounded-xl border border-dashed border-rule px-3 py-8 text-center text-xs text-mute">No conversations</li>
        ) : filtered.map((session) => (
          <li key={session.id} className={`group flex items-center gap-2 rounded-xl border px-2.5 py-2 transition ${activeId === session.id ? 'border-accent/35 bg-accent/10 shadow-panel' : 'border-transparent text-ink-2 hover:border-rule hover:bg-paper'}`}>
            <MessageSquareText size={14} className={activeId === session.id ? 'text-accent' : 'text-mute'} />
            <button type="button" onClick={() => onSwitch(session.id)} className="min-w-0 flex-1 text-left">
              <span className="block truncate text-xs font-medium text-ink">{session.title || 'New chat'}</span>
              <span className="mt-0.5 block truncate font-mono text-[9px] text-mute">{session.modelName || 'default model'}</span>
            </button>
            <button type="button" onClick={() => onDelete(session.id)} className="rounded-md p-1.5 text-mute opacity-0 transition hover:bg-critical/10 hover:text-critical group-hover:opacity-100 group-focus-within:opacity-100" aria-label={`Delete ${session.title}`}><Trash2 size={12} /></button>
          </li>
        ))}
      </ul>
    </div>
  );
}
