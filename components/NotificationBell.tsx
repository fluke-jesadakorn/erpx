import React from 'react';
import { listUserNotifications, listUnreadCount } from '@/notifications/queries';
import { reconcileOpenActionsForUser } from '@/notifications/waybill';
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
  const { loadActor } = await import('@/server/guard');
  const actor = await loadActor();
  if (!actor) return null;
  await reconcileOpenActionsForUser(actor.id);

  const [items, unread] = await Promise.all([
    listUserNotifications(actor.id, 15, { view: 'all' }),
    listUnreadCount(actor.id),
  ]);

  return (
    <div className="flex items-center gap-2">
      <NotificationDigestCard />
      <NotificationBellClient
        initialItems={items}
        unread={unread}
        hideButton={!!hideButton}
        scoped
      />
    </div>
  );
}
