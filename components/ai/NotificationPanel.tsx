'use client';

import React, { useState } from 'react';
import { T } from '@/components/i18n/T';

export interface NotificationItem {
  id: string | number;
  type: string;
  message: string;
  createdAt: string;
  severityClass?: string;
  readAt?: string | null;
}

export interface NotificationPanelProps {
  items: NotificationItem[];
  onClose: () => void;
  onToggleRead?: (id: string | number, currentlyRead: boolean) => void;
  onClear?: (id: string | number) => void;
  onMarkAllRead?: () => void;
  onClearAll?: () => void;
  filter?: 'all' | 'unread';
  onFilterChange?: (next: 'all' | 'unread') => void;
  scoped?: boolean;
}

const GLYPH_BY_TYPE: Record<string, string> = {
  'expense.submitted': '🧾',
  'expense.advanced': '🛡️',
  'expense.paid': '💳',
  'expense.rejected': '⛔',
  'ceo.override': '⚡',
  'pr.submitted': '🛒',
  'pr.advanced': '🛒',
  'pr.rejected': '⛔',
  'po.created': '📦',
  'po.advanced': '📦',
  'po.settled': '✅',
  'policy.updated': '⚙️',
};

function relTime(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime();
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  return `${d}d ago`;
}

export const NotificationPanel: React.FC<NotificationPanelProps> = ({
  items,
  onClose,
  onToggleRead,
  onClear,
  onMarkAllRead,
  onClearAll,
  filter = 'all',
  onFilterChange,
  scoped = false,
}) => {
  const [hoverId, setHoverId] = useState<string | number | null>(null);
  const visible = filter === 'unread' ? items.filter((it) => !it.readAt) : items;
  const unreadCount = items.filter((it) => !it.readAt).length;

  return (
    <>
      <div className="fixed inset-0 z-40" onClick={onClose} aria-hidden />
      <div className="absolute right-0 top-full mt-2 w-96 z-50 glass-panel-heavy rounded-2xl shadow-2xl shadow-black border border-slate-800 p-3 max-h-[70vh] flex flex-col animate-fade-in">
        <div className="flex items-center justify-between mb-2 px-1">
          <div className="flex items-center gap-2">
            <div className="text-xs font-mono uppercase tracking-wide text-slate-500">
              <T id="ai.notification.title" />
            </div>
            <span className="text-xs font-mono px-1.5 py-0.5 rounded-full bg-slate-800 text-slate-400">
              {scoped ? <T id="ai.notification.scopeMine" /> : <T id="ai.notification.scopeAll" />}
            </span>
            {unreadCount > 0 && (
              <span className="text-xs font-mono px-1.5 py-0.5 rounded-full bg-rose-500/20 text-rose-300 border border-rose-500/30">
                {unreadCount} <T id="ai.notification.new" />
              </span>
            )}
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close notifications"
            title="Close"
            className="w-5 h-5 inline-flex items-center justify-center rounded text-slate-500 hover:text-white hover:bg-slate-800 text-xs"
          >
            ✕
          </button>
        </div>

        {scoped && (
          <div className="flex items-center gap-1 mb-2 px-1">
            {(['all', 'unread'] as const).map((f) => (
              <button
                key={f}
                type="button"
                onClick={() => onFilterChange?.(f)}
                className={[
                  'px-2 py-1 rounded-md text-xs font-mono uppercase tracking-wide transition-colors',
                  filter === f
                    ? 'bg-indigo-500/20 text-indigo-200 border border-indigo-500/40'
                    : 'bg-slate-900/40 text-slate-400 border border-transparent hover:text-white hover:border-slate-700',
                ].join(' ')}
              >
                {f}
              </button>
            ))}
            {onMarkAllRead && (
              <button
                type="button"
                onClick={onMarkAllRead}
                className="ml-auto text-xs font-mono text-slate-400 hover:text-white"
              >
                <T id="ai.notification.markAllRead" />
              </button>
            )}
            {onClearAll && (
              <button
                type="button"
                onClick={onClearAll}
                className="text-xs font-mono text-rose-300 hover:text-rose-200"
              >
                <T id="ai.notification.clearAll" />
              </button>
            )}
          </div>
        )}

        <div className="overflow-y-auto flex-1 -mx-1 px-1">
          {visible.length === 0 ? (
            <div className="py-8 text-center text-xs text-slate-500 font-mono">
              <T id="ai.notification.empty" />
            </div>
          ) : (
            <ul className="space-y-1">
              {visible.map((it) => {
                const isUnread = !it.readAt;
                return (
                  <li
                    key={it.id}
                    onMouseEnter={() => setHoverId(it.id)}
                    onMouseLeave={() => setHoverId(null)}
                    className={[
                      'rounded-lg border px-3 py-2 text-xs',
                      isUnread
                        ? 'bg-indigo-500/10 border-indigo-500/40 text-white'
                        : 'bg-slate-900/40 border-slate-800 text-slate-300',
                    ].join(' ')}
                  >
                    <div className="flex items-start gap-2">
                      <span className="text-base leading-none mt-0.5">{GLYPH_BY_TYPE[it.type] ?? '🔔'}</span>
                      <div className="flex-1 min-w-0">
                        <div className="break-words">{it.message}</div>
                        <div className="mt-1 flex items-center gap-2 text-[10px] font-mono uppercase tracking-wide text-slate-500">
                          <span>{relTime(it.createdAt)}</span>
                          {onToggleRead && (
                            <button
                              type="button"
                              onClick={() => onToggleRead(it.id, !!it.readAt)}
                              className="hover:text-white"
                            >
                              <T id={isUnread ? 'ai.notification.markRead' : 'ai.notification.markUnread'} />
                            </button>
                          )}
                          {onClear && hoverId === it.id && (
                            <button
                              type="button"
                              onClick={() => onClear(it.id)}
                              className="text-rose-300 hover:text-rose-200"
                            >
                              <T id="ai.notification.clear" />
                            </button>
                          )}
                        </div>
                      </div>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>
    </>
  );
};