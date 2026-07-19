'use client';

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { Bell } from 'lucide-react';
import { NotificationPanel, type NotificationItem } from './NotificationPanel';

interface NotificationBellClientProps {
  initialItems: NotificationItem[];
  unread: number;
  hideButton: boolean;
  scoped: boolean;
}

const POLL_MS = 15_000;

export const NotificationBellClient: React.FC<NotificationBellClientProps> = ({
  initialItems,
  unread,
  hideButton,
  scoped,
}) => {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [items, setItems] = useState<NotificationItem[]>(initialItems);
  const [count, setCount] = useState(unread);
  const [filter, setFilter] = useState<'all' | 'unread'>('all');
  const sinceRef = useRef<string>(initialItems[0]?.createdAt ?? new Date(0).toISOString());

  useEffect(() => {
    const openNotifications = () => setOpen(true);
    window.addEventListener('folio:open-notifications', openNotifications);
    return () => window.removeEventListener('folio:open-notifications', openNotifications);
  }, []);

  useEffect(() => {
    if (hideButton || !scoped) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const tick = async () => {
      try {
        const response = await fetch(`/api/notifications?view=all&limit=30&since=${encodeURIComponent(sinceRef.current)}`, { cache: 'no-store' });
        if (!response.ok) throw new Error('notification polling failed');
        const data = await response.json() as { items?: NotificationItem[]; unread?: number };
        if (cancelled) return;
        const fresh = data.items ?? [];
        setItems((previous) => {
          const known = new Set(previous.map((item) => String(item.id)));
          return [...fresh.filter((item) => !known.has(String(item.id))), ...previous].slice(0, 50);
        });
        if (typeof data.unread === 'number') setCount(data.unread);
        if (fresh[0]) sinceRef.current = fresh[0].createdAt;
      } catch {
        // Polling is best-effort; the next tick retries.
      } finally {
        if (!cancelled) timer = setTimeout(tick, POLL_MS);
      }
    };

    timer = setTimeout(tick, POLL_MS);
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [hideButton, scoped]);

  const post = useCallback(async (path: string, body?: unknown) => {
    const response = await fetch(path, {
      method: 'POST',
      headers: body ? { 'content-type': 'application/json' } : undefined,
      body: body ? JSON.stringify(body) : undefined,
      cache: 'no-store',
    });
    if (!response.ok) throw new Error(`notification request failed: ${response.status}`);
    return response.json() as Promise<Record<string, unknown>>;
  }, []);

  const onOpen = useCallback(async (item: NotificationItem) => {
    try {
      const result = await post(`/api/notifications/${encodeURIComponent(item.id)}/open`);
      const opened = result.item as NotificationItem;
      setItems((previous) => previous.map((row) => String(row.id) === String(item.id) ? opened : row));
      setCount((value) => item.readAt ? value : Math.max(0, value - 1));
      if (typeof result.href === 'string') router.push(result.href);
    } catch {
      // Keep the panel open if the item disappeared or the request failed.
    }
  }, [post, router]);

  const onToggleRead = useCallback(async (id: string, currentlyRead: boolean) => {
    try {
      await post('/api/notifications/mark-read', { ids: [id], read: currentlyRead ? false : true });
      const readAt = currentlyRead ? null : new Date().toISOString();
      setItems((previous) => previous.map((item) => String(item.id) === String(id) ? { ...item, readAt } : item));
      setCount((value) => currentlyRead ? value + 1 : Math.max(0, value - 1));
    } catch {}
  }, [post]);

  const onDelete = useCallback(async (id: string) => {
    try {
      const response = await fetch(`/api/notifications/${encodeURIComponent(id)}`, { method: 'DELETE', cache: 'no-store' });
      if (!response.ok) return;
      setItems((previous) => previous.filter((item) => String(item.id) !== String(id)));
    } catch {}
  }, []);

  const onMarkAllRead = useCallback(async () => {
    try {
      await post('/api/notifications/mark-read', { all: true, read: true });
      const now = new Date().toISOString();
      setItems((previous) => previous.map((item) => ({ ...item, readAt: item.readAt ?? now })));
      setCount(0);
    } catch {}
  }, [post]);

  return (
    <div className="relative">
      {!hideButton && (
        <button
          type="button"
          aria-label="Notifications"
          onClick={() => setOpen((value) => !value)}
          className="relative inline-flex h-9 w-9 items-center justify-center rounded-lg border border-rule bg-paper-2 text-ink-2 transition-colors hover:border-rule-strong hover:bg-paper-3 hover:text-ink"
        >
          <Bell size={16} />
          {count > 0 && <span className="absolute -top-1 -right-1 inline-flex h-[17px] min-w-[17px] items-center justify-center rounded-full bg-critical px-1 text-[10px] font-semibold font-mono text-ink">{count > 99 ? '99+' : count}</span>}
        </button>
      )}
      {hideButton && count > 0 && <span className="absolute -top-1 -right-1 inline-flex h-[17px] min-w-[17px] items-center justify-center rounded-full bg-critical px-1 text-[10px] font-semibold font-mono text-ink">{count > 99 ? '99+' : count}</span>}
      {(hideButton || open) && (
        <NotificationPanel
          items={items}
          onClose={() => setOpen(false)}
          onOpen={onOpen}
          onToggleRead={onToggleRead}
          onDelete={onDelete}
          onMarkAllRead={onMarkAllRead}
          filter={filter}
          onFilterChange={setFilter}
          scoped={scoped}
        />
      )}
    </div>
  );
};
