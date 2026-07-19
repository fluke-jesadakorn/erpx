'use client';

import { useRouter } from 'next/navigation';
import { useState } from 'react';
import { Bell, Check, ExternalLink, Trash2, Zap } from 'lucide-react';

interface NotificationItem {
  id: string;
  message: string;
  category: 'action' | 'update';
  readAt: string | null;
  resolvedAt: string | null;
  resolvedBy: string | null;
  stageKey: string | null;
  waybillId: string | null;
  href: string | null;
  createdAt: string;
}

interface Props {
  initialItems: NotificationItem[];
}

function age(iso: string): string {
  const seconds = Math.max(0, Math.floor((Date.now() - new Date(iso).getTime()) / 1000));
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

export function NotificationInboxList({ initialItems }: Props) {
  const router = useRouter();
  const [items, setItems] = useState(initialItems);
  const [busy, setBusy] = useState<string | null>(null);

  async function open(item: NotificationItem) {
    setBusy(item.id);
    try {
      const response = await fetch(`/api/notifications/${encodeURIComponent(item.id)}/open`, { method: 'POST', cache: 'no-store' });
      if (!response.ok) return;
      const data = await response.json() as { item: NotificationItem; href?: string | null };
      setItems((previous) => previous.map((row) => row.id === item.id ? data.item : row));
      if (data.href) router.push(data.href);
    } catch {
      return;
    } finally {
      setBusy(null);
    }
  }

  async function toggleRead(item: NotificationItem) {
    const response = await fetch('/api/notifications/mark-read', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ ids: [item.id], read: !item.readAt }),
      cache: 'no-store',
    });
    if (!response.ok) return;
    setItems((previous) => previous.map((row) => row.id === item.id ? { ...row, readAt: item.readAt ? null : new Date().toISOString() } : row));
  }

  async function remove(item: NotificationItem) {
    if (item.category === 'action' && !item.resolvedAt) return;
    if (!window.confirm('Delete this notification permanently?')) return;
    const response = await fetch(`/api/notifications/${encodeURIComponent(item.id)}`, { method: 'DELETE', cache: 'no-store' });
    if (!response.ok) return;
    setItems((previous) => previous.filter((row) => row.id !== item.id));
  }

  if (items.length === 0) {
    return <div className="panel p-10 text-center text-sm text-mute">No notifications in this view.</div>;
  }

  return (
    <ul role="list" className="panel divide-y divide-rule overflow-hidden">
      {items.map((item) => {
        const unread = !item.readAt;
        const activeAction = item.category === 'action' && !item.resolvedAt;
        return (
          <li key={item.id} className={`flex items-start gap-3 px-4 py-4 ${unread ? 'bg-accent/10' : ''}`}>
            <span className={`mt-0.5 grid h-9 w-9 shrink-0 place-items-center rounded-lg border ${activeAction ? 'border-caution/50 bg-caution-soft text-caution' : 'border-rule bg-paper-2 text-ink-2'}`}>
              {activeAction ? <Zap size={16} aria-hidden /> : <Bell size={16} aria-hidden />}
            </span>
            <div className="min-w-0 flex-1">
              <button type="button" onClick={() => void open(item)} disabled={busy === item.id} className="block w-full text-left text-sm text-ink hover:text-info disabled:opacity-60">
                {item.message}
              </button>
              <div className="mt-1 flex flex-wrap items-center gap-2 text-xs font-mono text-mute">
                <span>{age(item.createdAt)}</span>
                {item.stageKey && <span>· {item.stageKey}</span>}
                {item.resolvedAt && <span>· handled{item.resolvedBy ? ` by ${item.resolvedBy}` : ''}</span>}
                {item.waybillId && <span>· {item.waybillId}</span>}
              </div>
            </div>
            <div className="flex shrink-0 items-center gap-1">
              {item.href && <ExternalLink size={13} className="text-mute" aria-hidden />}
              <button type="button" onClick={() => void toggleRead(item)} aria-label={unread ? 'Mark read' : 'Mark unread'} className="rounded p-1.5 text-mute hover:bg-paper-2 hover:text-ink">
                <Check size={14} aria-hidden />
              </button>
              {(item.category === 'update' || !!item.resolvedAt) && (
                <button type="button" onClick={() => void remove(item)} aria-label="Delete notification" className="rounded p-1.5 text-mute hover:bg-critical-soft hover:text-critical">
                  <Trash2 size={14} aria-hidden />
                </button>
              )}
            </div>
          </li>
        );
      })}
    </ul>
  );
}
