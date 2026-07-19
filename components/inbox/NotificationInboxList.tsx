'use client';

import { useRouter } from 'next/navigation';
import { useState } from 'react';
import { Bell } from 'lucide-react';
import { NotificationItemRow, type NotificationRowItem } from '@/components/ai/NotificationItemRow';
import { Empty } from '@/components/ui/Empty';
import { useToast } from '@/components/ui/Toast';

interface Props {
  initialItems: NotificationRowItem[];
}

export function NotificationInboxList({ initialItems }: Props) {
  const router = useRouter();
  const toast = useToast();
  const [items, setItems] = useState<NotificationRowItem[]>(initialItems);
  const [busy, setBusy] = useState<string | null>(null);

  async function open(item: NotificationRowItem) {
    setBusy(item.id);
    try {
      const response = await fetch(`/api/notifications/${encodeURIComponent(item.id)}/open`, { method: 'POST', cache: 'no-store' });
      if (!response.ok) return;
      const data = await response.json() as { item: NotificationRowItem; href?: string | null };
      setItems((previous) => previous.map((row) => row.id === item.id ? data.item : row));
      if (data.href) router.push(data.href);
    } catch {
      return;
    } finally {
      setBusy(null);
    }
  }

  async function toggleRead(item: NotificationRowItem) {
    const response = await fetch('/api/notifications/mark-read', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ ids: [item.id], read: !item.readAt }),
      cache: 'no-store',
    });
    if (!response.ok) return;
    setItems((previous) => previous.map((row) => row.id === item.id ? { ...row, readAt: item.readAt ? null : new Date().toISOString() } : row));
  }

  async function remove(item: NotificationRowItem) {
    if (item.category === 'action' && !item.resolvedAt) return;
    const response = await fetch(`/api/notifications/${encodeURIComponent(item.id)}`, { method: 'DELETE', cache: 'no-store' });
    if (!response.ok) return;
    setItems((previous) => previous.filter((row) => row.id !== item.id));
    toast.info('Notification deleted.');
  }

  if (items.length === 0) {
    return (
      <div className="panel p-6">
        <Empty
          icon={Bell}
          title="No notifications in this view"
          body="Try a different filter, or check back later."
        />
      </div>
    );
  }

  return (
    <div className="panel p-3">
      <div className="space-y-1.5">
        {items.map((item) => (
          <NotificationItemRow
            key={item.id}
            item={item}
            onOpen={busy === item.id ? undefined : open}
            onToggleRead={(notification) => toggleRead(notification)}
            onDelete={(notification) => remove(notification)}
            alwaysShowActions
          />
        ))}
      </div>
    </div>
  );
}
