import { NextResponse } from 'next/server';
import { requireActor } from '@/server/guard';
import {
  listActionCount,
  listUnreadCount,
  listUserNotifications,
  type NotificationReadFilter,
  type NotificationView,
} from '@/notifications/queries';
import { reconcileOpenActionsForUser } from '@/notifications/waybill';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function viewFromUrl(url: URL): NotificationView {
  const raw = url.searchParams.get('view');
  if (raw === 'actions' || raw === 'notifications') return raw;
  const scope = url.searchParams.get('scope');
  if (scope === 'waiting') return 'actions';
  if (scope === 'watching') return 'notifications';
  return 'all';
}

function readFromUrl(url: URL): NotificationReadFilter {
  const raw = url.searchParams.get('read');
  if (raw === 'unread' || raw === 'read') return raw;
  return url.searchParams.get('onlyUnread') === 'true' ? 'unread' : 'all';
}

export async function GET(req: Request) {
  const actor = await requireActor().catch(() => null);
  if (!actor) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  await reconcileOpenActionsForUser(actor.id);

  const url = new URL(req.url);
  const limit = Math.max(1, Math.min(100, Number(url.searchParams.get('limit') ?? 30) || 30));
  const domainRaw = url.searchParams.get('domain');
  const domain = domainRaw === 'expense' || domainRaw === 'so' || domainRaw === 'pr' || domainRaw === 'po' ? domainRaw : 'all';
  const items = await listUserNotifications(actor.id, limit, {
    view: viewFromUrl(url),
    read: readFromUrl(url),
    domain,
    watchingOnly: url.searchParams.get('source') === 'watching' || url.searchParams.get('scope') === 'watching',
    cursor: url.searchParams.get('cursor'),
    since: url.searchParams.get('since'),
  });
  return NextResponse.json({
    items,
    unread: await listUnreadCount(actor.id),
    actions: await listActionCount(actor.id),
    nextCursor: items.length === limit ? items[items.length - 1]?.id ?? null : null,
  });
}
