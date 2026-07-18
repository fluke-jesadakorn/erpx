'use client';

import React, { useCallback, useEffect, useRef, useState } from 'react';
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
  const [open, setOpen] = useState(false);
  const [items, setItems] = useState<NotificationItem[]>(initialItems);
  const [count, setCount] = useState<number>(unread);
  const [filter, setFilter] = useState<'all' | 'unread'>('all');
  const sinceRef = useRef<string>(
    initialItems.length ? initialItems[0].createdAt : new Date(0).toISOString(),
  );

  useEffect(() => {
    function onExternal() { setOpen(true); }
    window.addEventListener('folio:open-notifications', onExternal);
    return () => window.removeEventListener('folio:open-notifications', onExternal);
  }, []);

  useEffect(() => {
    if (hideButton) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    async function tick() {
      try {
        const url = scoped
          ? `/api/notifications?scope=mine&limit=15&since=${encodeURIComponent(sinceRef.current)}`
          : `/api/notifications?limit=15&since=${encodeURIComponent(sinceRef.current)}`;
        const res = await fetch(url, { cache: 'no-store' });
        if (!res.ok) throw new Error(`status ${res.status}`);
        const data = await res.json();
        const fresh: NotificationItem[] = data.items || [];
        if (cancelled) return;

        setItems((prev) => {
          const seen = new Set(prev.map((p) => String(p.id)));
          return [...fresh.filter((f) => !seen.has(String(f.id))), ...prev].slice(0, 30);
        });

        if (typeof data.unread === 'number') {
          setCount(data.unread);
        } else if (fresh.length > 0) {
          setCount((c) => c + fresh.length);
        }

        if (fresh.length > 0) sinceRef.current = fresh[0].createdAt;
      } catch {
        // ignore polling errors
      } finally {
        if (!cancelled) timer = setTimeout(tick, POLL_MS);
      }
    }

    timer = setTimeout(tick, POLL_MS);
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [hideButton, scoped]);

  async function postAction(path: string, body: unknown): Promise<{ ok?: boolean; updated?: number } | null> {
    try {
      const r = await fetch(path, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
        cache: 'no-store',
      });
      if (!r.ok) return null;
      return r.json();
    } catch {
      return null;
    }
  }

  const onToggleRead = useCallback(async (id: string | number, currentlyRead: boolean) => {
    const result = await postAction('/api/notifications/mark-read', { ids: [id] });
    if (!result) return;
    const readAt = new Date().toISOString();
    setItems((prev) =>
      prev.map((it) =>
        String(it.id) === String(id) ? { ...it, readAt } : it,
      ),
    );
    if (!currentlyRead) setCount((c) => Math.max(0, c - 1));
  }, []);

  const onClear = useCallback(async (id: string | number) => {
    const result = await postAction('/api/notifications/mark-read', { ids: [id] });
    if (!result) return;
    const readAt = new Date().toISOString();
    setItems((prev) =>
      prev.map((it) =>
        String(it.id) === String(id) ? { ...it, readAt } : it,
      ),
    );
    setCount((c) => Math.max(0, c - 1));
  }, []);

  const onMarkAllRead = useCallback(async () => {
    const ids = items.filter((it) => !it.readAt).map((it) => it.id);
    if (ids.length === 0) {
      const result = await postAction('/api/notifications/mark-read', { all: true });
      if (result) {
        setItems((prev) => prev.map((it) => ({ ...it, readAt: it.readAt ?? new Date().toISOString() })));
        setCount(0);
      }
      return;
    }
    const result = await postAction('/api/notifications/mark-read', { ids });
    if (result) {
      const now = new Date().toISOString();
      setItems((prev) => prev.map((it) => (it.readAt ? it : { ...it, readAt: now })));
      setCount(0);
    }
  }, [items]);

  const onClearAll = useCallback(async () => {
    const result = await postAction('/api/notifications/mark-read', { all: true });
    if (result) {
      setItems([]);
      setCount(0);
    }
  }, []);

  const list = items;
  const unreadCount = count;

  return (
    <div className="relative">
      {!hideButton && (
        <button
          type="button"
          aria-label="Notifications"
          onClick={() => setOpen((v) => !v)}
          className="relative inline-flex h-9 w-9 items-center justify-center rounded-lg border border-rule bg-paper-2 text-ink-2 transition-colors hover:border-rule-strong hover:bg-paper-3 hover:text-ink"
        >
          <Bell size={16} />
          {unreadCount > 0 && (
            <span className="absolute -top-1 -right-1 inline-flex h-[17px] min-w-[17px] items-center justify-center rounded-full bg-critical px-1 text-[10px] font-semibold font-mono text-ink">
              {unreadCount > 99 ? '99+' : unreadCount}
            </span>
          )}
        </button>
      )}

      {hideButton && unreadCount > 0 && (
        <span className="absolute -top-1 -right-1 inline-flex h-[17px] min-w-[17px] items-center justify-center rounded-full bg-critical px-1 text-[10px] font-semibold font-mono text-ink">
          {unreadCount > 99 ? '99+' : unreadCount}
        </span>
      )}

      {hideButton ? (
        <NotificationPanel
          items={list}
          onClose={() => {}}
          onToggleRead={onToggleRead}
          onClear={onClear}
          onMarkAllRead={onMarkAllRead}
          onClearAll={onClearAll}
          filter={filter}
          onFilterChange={setFilter}
          scoped={scoped}
        />
      ) : (
        open && (
          <NotificationPanel
            items={list}
            onClose={() => setOpen(false)}
            onToggleRead={onToggleRead}
            onClear={onClear}
            onMarkAllRead={onMarkAllRead}
            onClearAll={onClearAll}
            filter={filter}
            onFilterChange={setFilter}
            scoped={scoped}
          />
        )
      )}
    </div>
  );
};