import { NextRequest, NextResponse } from 'next/server';
import { replayHookEvent } from '@/hook/replay';
import { loadActor } from '@/server/guard';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: NextRequest, ctx: { params: Promise<{ id: string }> }) {
  const actor = await loadActor();
  const session = await loadActivePermSession(req);
  if (!actor || !session) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const allowed = hasPermission(session.session, PERM.hook.event.replay);
  if (!allowed) return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id } = await ctx.params;
  const eventId = Number(id);
  if (!Number.isFinite(eventId) || eventId <= 0) {
    return NextResponse.json({ error: 'invalid id' }, { status: 400 });
  }
  const out = await replayHookEvent(eventId, String(actor.id));
  return NextResponse.json(out);
}