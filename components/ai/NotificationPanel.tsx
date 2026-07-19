'use client';

import React, { useState } from 'react';
import { T } from '@/components/i18n/T';

export interface NotificationItem {
  id: string;
  type: string;
  category: 'action' | 'update';
  message: string;
  href?: string | null;
  createdAt: string;
  readAt?: string | null;
  resolvedAt?: string | null;
  resolvedBy?: string | null;
  audience?: 'owner' | 'approver' | 'watcher';
}

export interface NotificationPanelProps {
  items: NotificationItem[];
  onClose: () => void;
  onOpen?: (item: NotificationItem) => void;
  onToggleRead?: (id: string, currentlyRead: boolean) => void;
  onDelete?: (id: string) => void;
  onMarkAllRead?: () => void;
  filter?: 'all' | 'unread';
  onFilterChange?: (next: 'all' | 'unread') => void;
  scoped?: boolean;
}

const GLYPH_BY_TYPE: Record<string, string> = {
  'waybill.expense.submitted': '🧾',
  'waybill.expense.advanced': '🛡️',
  'waybill.expense.payment-confirmed': '💳',
  'waybill.expense.rejected': '⛔',
  'waybill.so.so-submitted': '💼',
  'waybill.so.so-reviewed': '🛡️',
  'waybill.so.so-credit-checked': '🔍',
  'waybill.so.so-invoiced': '🧾',
  'waybill.so.so-paid': '💰',
  'waybill.so.so-rejected': '⛔',
};

function relTime(iso: string): string {
  const seconds = Math.max(0, Math.floor((Date.now() - new Date(iso).getTime()) / 1000));
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

export const NotificationPanel: React.FC<NotificationPanelProps> = ({
  items,
  onClose,
  onOpen,
  onToggleRead,
  onDelete,
  onMarkAllRead,
  filter = 'all',
  onFilterChange,
  scoped = false,
}) => {
  const [hoverId, setHoverId] = useState<string | null>(null);
  const visible = filter === 'unread' ? items.filter((item) => !item.readAt) : items;
  const unreadCount = items.filter((item) => !item.readAt).length;

  return (
    <>
      <div className="fixed inset-0 z-sticky" onClick={onClose} aria-hidden />
      <div className="absolute right-0 top-full z-fixed mt-2 flex max-h-[70vh] w-96 max-w-[calc(100vw-1rem)] flex-col rounded-xl border border-rule-strong bg-paper p-4 shadow-modal animate-fade-in">
        <div className="mb-3 flex items-center justify-between px-1">
          <div className="flex items-center gap-2">
            <div className="text-xs font-mono uppercase tracking-wide text-mute"><T id="ai.notification.title" /></div>
            {scoped && <span className="text-xs font-mono text-ink-2"><T id="ai.notification.scopeMine" /></span>}
            {unreadCount > 0 && <span className="rounded-full border border-critical bg-critical px-1.5 py-0.5 text-xs font-mono text-critical">{unreadCount} <T id="ai.notification.new" /></span>}
          </div>
          <button type="button" onClick={onClose} aria-label="Close notifications" className="inline-flex h-5 w-5 items-center justify-center rounded text-mute hover:bg-paper-2 hover:text-ink">✕</button>
        </div>

        {scoped && (
          <div className="mb-3 flex items-center gap-1 px-1">
            {(['all', 'unread'] as const).map((value) => (
              <button key={value} type="button" onClick={() => onFilterChange?.(value)} className={`rounded-md px-2 py-1 text-xs font-mono uppercase tracking-wide ${filter === value ? 'bg-accent text-paper' : 'text-ink-2 hover:text-ink'}`}>{value}</button>
            ))}
            {onMarkAllRead && <button type="button" onClick={onMarkAllRead} className="ml-auto text-xs font-mono text-ink-2 hover:text-ink"><T id="ai.notification.markAllRead" /></button>}
          </div>
        )}

        <div className="-mx-1 flex-1 overflow-y-auto px-1">
          {visible.length === 0 ? (
            <div className="py-8 text-center text-xs font-mono text-mute"><T id="ai.notification.empty" /></div>
          ) : (
            <ul className="space-y-1">
              {visible.map((item) => {
                const unread = !item.readAt;
                const deletable = item.category === 'update' || !!item.resolvedAt;
                return (
                  <li key={item.id} onMouseEnter={() => setHoverId(item.id)} onMouseLeave={() => setHoverId(null)} className={`rounded-lg border px-3.5 py-3 text-xs ${unread ? 'border-action bg-action text-action-ink' : 'border-rule bg-paper-2 text-ink-2'}`}>
                    <div className="flex items-start gap-2">
                      <span className="mt-0.5 text-base leading-none">{GLYPH_BY_TYPE[item.type] ?? (item.category === 'action' ? '⚡' : '🔔')}</span>
                      <div className="min-w-0 flex-1">
                        <button type="button" onClick={() => onOpen?.(item)} className="block w-full break-words text-left text-sm font-medium leading-6 hover:underline">{item.message}</button>
                        <div className={`mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[10px] font-mono uppercase tracking-wide ${unread ? 'text-action-ink' : 'text-mute'}`}>
                          <span>{relTime(item.createdAt)}</span>
                          {item.resolvedAt && <span>· {item.resolvedBy ? `handled by ${item.resolvedBy}` : 'handled'}</span>}
                          {onToggleRead && (
                            <button type="button" onClick={() => onToggleRead(item.id, !!item.readAt)} className={unread ? 'text-action-ink hover:text-action-ink' : 'text-ink-2 hover:text-ink'}>
                              <T
                                id={unread ? 'ai.notification.markRead' : 'ai.notification.markUnread'}
                                primaryClassName={unread ? 'font-semibold text-action-ink' : 'font-semibold text-ink-2'}
                                secondaryClassName={unread ? 'ml-1.5 text-[10px] font-semibold text-action-ink' : 'ml-1.5 text-[10px] font-normal text-ink-2'}
                              />
                            </button>
                          )}
                          {onDelete && deletable && hoverId === item.id && (
                            <button type="button" onClick={() => onDelete(item.id)} className={unread ? 'text-action-ink hover:text-action-ink' : 'text-critical hover:text-critical-strong'}>
                              <T
                                id="ai.notification.delete"
                                primaryClassName={unread ? 'font-semibold text-action-ink' : 'font-semibold text-critical'}
                                secondaryClassName={unread ? 'ml-1.5 text-[10px] font-semibold text-action-ink' : 'ml-1.5 text-[10px] font-normal text-critical'}
                              />
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
