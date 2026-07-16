import { NextResponse } from 'next/server';
import { requireActor } from '@/server/guard';
import {
  listRecentNotifications,
  listUserNotifications,
  listUnreadCount,
} from '@/notifications/queries';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const url = new URL(req.url);
  const scope = url.searchParams.get('scope');
  const unreadOnly = url.searchParams.get('onlyUnread') === 'true';
  const includeCleared = url.searchParams.get('includeCleared') === 'true';
  const limitRaw = url.searchParams.get('limit');
  const sinceRaw = url.searchParams.get('since');
  const limit = Math.max(1, Math.min(100, parseInt(limitRaw || '15', 10) || 15));

  if (scope === 'mine') {
    const actor = await requireActor().catch(() => null);
    if (!actor) {
      return NextResponse.json({ items: [], unread: 0 });
    }
    const sinceMs = sinceRaw ? Date.parse(sinceRaw) : NaN;
    let rows = await listUserNotifications(actor.id, limit, { includeCleared, onlyUnread: unreadOnly });
    if (!Number.isNaN(sinceMs)) {
      rows = rows.filter((it) => Date.parse(it.createdAt) > sinceMs);
    }
    const unread = await listUnreadCount(actor.id);
    return NextResponse.json({ items: rows, unread });
  }

  if (scope === 'unread') {
    const actor = await requireActor().catch(() => null);
    if (!actor) return NextResponse.json({ unread: 0 });
    return NextResponse.json({ unread: await listUnreadCount(actor.id) });
  }

  let items = await listRecentNotifications(limit);
  if (sinceRaw) {
    const sinceMs = Date.parse(sinceRaw);
    if (!Number.isNaN(sinceMs)) {
      items = items.filter((it) => Date.parse(it.createdAt) > sinceMs);
    }
  }
  return NextResponse.json({ items });
}