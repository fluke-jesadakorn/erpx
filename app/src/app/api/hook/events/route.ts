import { NextRequest, NextResponse } from 'next/server';
import { listHookEvents } from '@folio-lib/hook/persist';
import { loadActor } from '@folio-lib/server/guard';
import { matchPerm } from '@folio-lib/perm';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: NextRequest) {
  const actor = await loadActor();
  if (!actor) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const allowed = matchPerm(actor.permissions, "hook:event:view::allow");
  if (!allowed) return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { searchParams } = new URL(req.url);
  const status = (searchParams.get('status') ?? undefined) as 'received' | 'processed' | 'failed' | 'rejected' | undefined;
  const providerId = searchParams.get('provider_id') ?? undefined;
  const limit = Number(searchParams.get('limit') ?? '50');

  const events = await listHookEvents({ providerId, status, limit });
  return NextResponse.json({ events });
}