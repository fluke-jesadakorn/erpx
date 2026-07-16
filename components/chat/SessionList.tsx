'use client';
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
  return (
    <div className="flex h-full flex-col gap-2 text-sm">
      <button
        type="button"
        onClick={onNew}
        className="rounded-lg bg-indigo-600 px-3 py-2 text-sm font-medium text-white hover:bg-indigo-500"
      >
        {newLabel}
      </button>
      <div className="px-1 text-xs font-mono uppercase tracking-widest text-slate-500">
        {sessionsLabel}
      </div>
      <ul className="flex-1 space-y-1 overflow-y-auto">
        {sessions.length === 0 ? (
          <li className="px-2 py-1 text-xs text-slate-500 font-mono">—</li>
        ) : (
          sessions.map((s) => (
            <li
              key={s.id}
              className={`group flex items-center gap-1 rounded-lg px-2 py-1.5 ${
                activeId === s.id ? 'bg-indigo-500/15 text-indigo-200' : 'text-slate-300 hover:bg-slate-900'
              }`}
            >
              <button
                type="button"
                onClick={() => onSwitch(s.id)}
                className="flex-1 truncate text-left"
              >
                {s.title || 'New chat'}
              </button>
              <button
                type="button"
                onClick={() => onDelete(s.id)}
                className="opacity-0 group-hover:opacity-100 text-slate-500 hover:text-rose-300"
                aria-label="delete"
              >
                ✕
              </button>
            </li>
          ))
        )}
      </ul>
    </div>
  );
}