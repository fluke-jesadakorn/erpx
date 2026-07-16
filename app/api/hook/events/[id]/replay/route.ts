import { NextRequest, NextResponse } from 'next/server';
import { replayHookEvent } from '@/hook/replay';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(_req: NextRequest, ctx: { params: Promise<{ id: string }> }) {
  const actor = await loadActor();
  if (!actor) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const allowed = matchPerm(actor.permissions, "hook:event:replay::allow");
  if (!allowed) return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id } = await ctx.params;
  const eventId = Number(id);
  if (!Number.isFinite(eventId) || eventId <= 0) {
    return NextResponse.json({ error: 'invalid id' }, { status: 400 });
  }
  const out = await replayHookEvent(eventId, String(actor.id));
  return NextResponse.json(out);
}