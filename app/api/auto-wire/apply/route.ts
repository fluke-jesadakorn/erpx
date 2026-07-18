import { NextRequest, NextResponse } from 'next/server';
import { loadActor } from '@/server/guard';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: NextRequest) {
  const actor = await loadActor();
  const session = await loadActivePermSession(req);
  if (!actor || !session) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const allowed = hasPermission(session.session, PERM.org.auto_wire.apply);
  if (!allowed) return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  const body = await req.json();
  if (!Array.isArray(body?.wires) || body.wires.length === 0) {
    return NextResponse.json({ error: 'No wires provided' }, { status: 400 });
  }
  const { applyAutoWire } = await import('@/perm/autoWire');
  const out = await applyAutoWire(body.wires);
  return NextResponse.json(out);
}