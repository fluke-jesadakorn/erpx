import React from 'react';
import { listRecentNotifications, listUserNotifications, listUnreadCount } from '@folio-lib/notifications/queries';
import { NotificationBellClient } from '@/components/ai/NotificationBellClient';
import { NotificationDigestCard } from './ai/NotificationDigestCard';

interface NotificationBellProps {
  userNameById?: Record<number, string>;
  hideButton?: boolean;
}

export async function NotificationBell({
  userNameById: _userNameById,
  hideButton,
}: NotificationBellProps = {}) {
  const { loadActor } = await import('@folio-lib/server/guard');
  const actor = await loadActor();

  const items = actor
    ? await listUserNotifications(actor.id, 15)
    : await listRecentNotifications(15);
  const unread = actor ? await listUnreadCount(actor.id) : items.length;

  return (
    <div className="flex items-center gap-2">
      <NotificationDigestCard />
      <NotificationBellClient
        initialItems={items.map((it: any) => ({
          id: it.id,
          type: it.type,
          message: it.message,
          createdAt: it.createdAt,
          severityClass: it.severityClass,
          readAt: it.readAt ?? null,
        }))}
        unread={unread}
        hideButton={!!hideButton}
        scoped={!!actor}
      />
    </div>
  );
}