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
      <div className="fixed inset-0 z-sticky" onClick={onClose} aria-hidden />
      <div className="absolute right-0 top-full mt-2 w-96 z-fixed bg-paper-2 rounded-md shadow-modal border border-rule p-3 max-h-[70vh] flex flex-col animate-fade-in">
        <div className="flex items-center justify-between mb-2 px-1">
          <div className="flex items-center gap-2">
            <div className="text-xs font-mono uppercase tracking-wide text-mute">
              <T id="ai.notification.title" />
            </div>
            <span className="text-xs font-mono px-1.5 py-0.5 rounded-full bg-paper-2 text-ink-2">
              {scoped ? <T id="ai.notification.scopeMine" /> : <T id="ai.notification.scopeAll" />}
            </span>
            {unreadCount > 0 && (
              <span className="text-xs font-mono px-1.5 py-0.5 rounded-full bg-critical text-critical border border-critical">
                {unreadCount} <T id="ai.notification.new" />
              </span>
            )}
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close notifications"
            title="Close"
            className="w-5 h-5 inline-flex items-center justify-center rounded text-mute hover:text-ink hover:bg-paper-2 text-xs"
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
                    ? 'bg-accent text-paper border border-accent'
                    : 'bg-paper-2/40 text-ink-2 border border-transparent hover:text-ink hover:border-rule',
                ].join(' ')}
              >
                {f}
              </button>
            ))}
            {onMarkAllRead && (
              <button
                type="button"
                onClick={onMarkAllRead}
                className="ml-auto text-xs font-mono text-ink-2 hover:text-ink"
              >
                <T id="ai.notification.markAllRead" />
              </button>
            )}
            {onClearAll && (
              <button
                type="button"
                onClick={onClearAll}
                className="text-xs font-mono text-critical hover:text-critical-soft"
              >
                <T id="ai.notification.clearAll" />
              </button>
            )}
          </div>
        )}

        <div className="overflow-y-auto flex-1 -mx-1 px-1">
          {visible.length === 0 ? (
            <div className="py-8 text-center text-xs text-mute font-mono">
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
                        ? 'bg-accent border-accent text-ink'
                        : 'bg-paper-2/40 border-rule text-ink-2',
                    ].join(' ')}
                  >
                    <div className="flex items-start gap-2">
                      <span className="text-base leading-none mt-0.5">{GLYPH_BY_TYPE[it.type] ?? '🔔'}</span>
                      <div className="flex-1 min-w-0">
                        <div className="break-words">{it.message}</div>
                        <div className="mt-1 flex items-center gap-2 text-[10px] font-mono uppercase tracking-wide text-mute">
                          <span>{relTime(it.createdAt)}</span>
                          {onToggleRead && (
                            <button
                              type="button"
                              onClick={() => onToggleRead(it.id, !!it.readAt)}
                              className="hover:text-ink"
                            >
                              <T id={isUnread ? 'ai.notification.markRead' : 'ai.notification.markUnread'} />
                            </button>
                          )}
                          {onClear && hoverId === it.id && (
                            <button
                              type="button"
                              onClick={() => onClear(it.id)}
                              className="text-critical hover:text-critical-soft"
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