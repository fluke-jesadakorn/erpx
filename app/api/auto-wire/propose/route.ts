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
  const { proposeAutoWire } = await import('@/perm/autoWire');
  const proposal = await proposeAutoWire();
  return NextResponse.json({ proposal });
}