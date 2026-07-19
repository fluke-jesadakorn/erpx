'use client';

import React from 'react';
import Link from 'next/link';
import { Bell, CheckCheck, Settings2, X } from 'lucide-react';
import { T } from '@/components/i18n/T';
import { Empty } from '@/components/ui/Empty';
import {
  NotificationItemRow,
  BUCKET_ORDER,
  bucketOf,
  relTime,
  type NotificationRowItem,
  type TimeBucket,
} from './NotificationItemRow';

export type { NotificationRowItem as NotificationItem };

export interface NotificationPanelProps {
  items: NotificationRowItem[];
  onClose: () => void;
  onOpen?: (item: NotificationRowItem) => void;
  onToggleRead?: (item: NotificationRowItem) => void;
  onDelete?: (item: NotificationRowItem) => void;
  onMarkAllRead?: () => void;
  filter?: 'all' | 'unread';
  onFilterChange?: (next: 'all' | 'unread') => void;
  scoped?: boolean;
  lastUpdatedAt?: string;
}

const BUCKET_LABEL: Record<TimeBucket, string> = {
  new: 'ai.notification.bucket.new',
  today: 'ai.notification.bucket.today',
  yesterday: 'ai.notification.bucket.yesterday',
  thisWeek: 'ai.notification.bucket.thisWeek',
  earlier: 'ai.notification.bucket.earlier',
};

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
  lastUpdatedAt,
}) => {
  const visible = filter === 'unread' ? items.filter((item) => !item.readAt) : items;
  const unreadCount = items.filter((item) => !item.readAt).length;

  const grouped = visible.reduce<Record<TimeBucket, NotificationRowItem[]>>(
    (acc, item) => {
      const b = bucketOf(item.createdAt);
      (acc[b] ??= []).push(item);
      return acc;
    },
    { new: [], today: [], yesterday: [], thisWeek: [], earlier: [] },
  );

  return (
    <>
      <div
        className="fixed inset-0 z-popover bg-canvas/70 backdrop-blur-[2px] animate-fade-in"
        onClick={onClose}
        aria-hidden
      />
      <div
        role="dialog"
        aria-label="Notifications"
        className="panel-floating absolute right-0 top-full z-popover mt-2 flex max-h-[min(72vh,680px)] w-[420px] max-w-[calc(100vw-1rem)] flex-col overflow-hidden rounded-2xl animate-fade-scale"
      >
        {/* Header */}
        <div className="flex items-start gap-3 border-b border-rule/60 px-4 pt-3.5 pb-3">
          <span className="grid h-9 w-9 shrink-0 place-items-center rounded-xl border border-accent/35 bg-accent-soft/60 text-accent">
            <Bell size={16} aria-hidden />
          </span>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <h2 className="text-sm font-semibold tracking-tight text-ink">
                <T id="ai.notification.title" hideSecondary />
              </h2>
              {unreadCount > 0 && (
                <span className="inline-flex h-5 min-w-[20px] items-center justify-center rounded-full bg-critical px-1.5 text-[10px] font-mono font-bold text-paper">
                  {unreadCount > 99 ? '99+' : unreadCount}
                </span>
              )}
            </div>
            <div className="mt-0.5 truncate text-[11px] font-mono text-mute" suppressHydrationWarning>
              {lastUpdatedAt ? (
                <T id="ai.notification.updated" values={{ ago: relTime(lastUpdatedAt) }} hideSecondary />
              ) : (
                <T id="ai.notification.scopeMine" hideSecondary />
              )}
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-0.5">
            {onMarkAllRead && (
              <button
                type="button"
                onClick={onMarkAllRead}
                disabled={unreadCount === 0}
                aria-label="Mark all as read"
                className="inline-flex h-8 w-8 items-center justify-center rounded-lg text-mute transition-colors hover:bg-paper-2/80 hover:text-ink disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-mute"
              >
                <CheckCheck size={15} aria-hidden />
              </button>
            )}
            <Link
              href="/inbox"
              onClick={onClose}
              aria-label="Open inbox"
              className="inline-flex h-8 w-8 items-center justify-center rounded-lg text-mute transition-colors hover:bg-paper-2/80 hover:text-ink"
            >
              <Settings2 size={15} aria-hidden />
            </Link>
            <button
              type="button"
              onClick={onClose}
              aria-label="Close notifications"
              className="inline-flex h-8 w-8 items-center justify-center rounded-lg text-mute transition-colors hover:bg-paper-2/80 hover:text-ink"
            >
              <X size={15} aria-hidden />
            </button>
          </div>
        </div>

        {/* Filter segmented control */}
        {scoped && (
          <div className="flex items-center gap-1 px-4 py-2.5">
            <div className="inline-flex items-center gap-0.5 rounded-lg border border-rule bg-paper-2/60 p-0.5">
              {(['all', 'unread'] as const).map((value) => {
                const active = filter === value;
                const label = value === 'all' ? (
                  <T id="ai.notification.filterAll" hideSecondary />
                ) : (
                  <T id="ai.notification.filterUnread" hideSecondary />
                );
                return (
                  <button
                    key={value}
                    type="button"
                    onClick={() => onFilterChange?.(value)}
                    className={[
                      'inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-[11px] font-mono uppercase tracking-wide transition-colors',
                      active
                        ? 'bg-accent text-paper shadow-sm'
                        : 'text-ink-2 hover:text-ink',
                    ].join(' ')}
                  >
                    {label}
                    {value === 'unread' && unreadCount > 0 && (
                      <span className={`rounded-full px-1.5 text-[10px] ${active ? 'bg-paper/25 text-paper' : 'bg-critical-soft/70 text-critical'}`}>
                        {unreadCount > 99 ? '99+' : unreadCount}
                      </span>
                    )}
                  </button>
                );
              })}
            </div>
            {onMarkAllRead && unreadCount > 0 && (
              <button
                type="button"
                onClick={onMarkAllRead}
                className="ml-auto text-[11px] font-mono text-ink-2 transition-colors hover:text-accent"
              >
                <T id="ai.notification.markAllRead" hideSecondary />
              </button>
            )}
          </div>
        )}

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-3 pb-2">
          {visible.length === 0 ? (
            <div className="px-2 py-4">
              <Empty
                icon={Bell}
                title={<T id="ai.notification.emptyTitle" hideSecondary />}
                body={<T id="ai.notification.emptyBody" hideSecondary />}
                action={{ label: <T id="ai.notification.emptyCta" hideSecondary />, href: '/inbox' }}
              />
            </div>
          ) : (
            <div className="space-y-4 py-1">
              {BUCKET_ORDER.map((bucket) => {
                const rows = grouped[bucket];
                if (!rows || rows.length === 0) return null;
                return (
                  <section key={bucket} className="space-y-1.5">
                    <div className="flex items-center gap-2 px-1">
                      <h3 className="text-[10px] font-mono font-semibold uppercase tracking-[0.16em] text-mute">
                        <T id={BUCKET_LABEL[bucket]} hideSecondary />
                      </h3>
                      <span className="rounded-full border border-rule bg-paper-2/60 px-1.5 py-0.5 text-[10px] font-mono text-ink-2">
                        {rows.length}
                      </span>
                      <span className="h-px flex-1 bg-rule/50" />
                    </div>
                    <div className="space-y-1.5">
                      {rows.map((item) => (
                        <NotificationItemRow
                          key={item.id}
                          item={item}
                          onOpen={onOpen}
                          onToggleRead={onToggleRead}
                          onDelete={onDelete}
                          compact
                        />
                      ))}
                    </div>
                  </section>
                );
              })}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between border-t border-rule/60 bg-paper-2/40 px-4 py-2.5">
          <Link
            href="/inbox"
            onClick={onClose}
            className="inline-flex items-center gap-1.5 text-xs font-medium text-ink-2 transition-colors hover:text-accent"
          >
            <T id="ai.notification.viewAll" hideSecondary />
            <span aria-hidden>→</span>
          </Link>
          <span className="text-[10px] font-mono uppercase tracking-wide text-mute">
            {items.length} {items.length === 1 ? 'item' : 'items'}
          </span>
        </div>
      </div>
    </>
  );
};

export default NotificationPanel;
