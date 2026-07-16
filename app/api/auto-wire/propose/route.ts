import { NextRequest, NextResponse } from 'next/server';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(_req: NextRequest) {
  const actor = await loadActor();
  if (!actor) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const allowed = matchPerm(actor.permissions, "org:auto_wire:apply::allow");
  if (!allowed) return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  const { proposeAutoWire } = await import('@/perm/autoWire');
  const proposal = await proposeAutoWire();
  return NextResponse.json({ proposal });
}