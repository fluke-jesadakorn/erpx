import { NextResponse } from 'next/server';
import { requireActor } from '@/server/guard';
import { openNotification } from '@/notifications/queries';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(
  _req: Request,
  { params }: { params: Promise<{ id: string }> },
) {
  const actor = await requireActor().catch(() => null);
  if (!actor) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const id = Number((await params).id);
  if (!Number.isInteger(id) || id <= 0) return NextResponse.json({ error: 'invalid id' }, { status: 400 });
  const item = await openNotification(actor.id, id);
  if (!item) return NextResponse.json({ error: 'not found' }, { status: 404 });
  return NextResponse.json({ ok: true, item, href: item.href });
}
