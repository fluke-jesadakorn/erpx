import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { loadProjection } from '@/server/projection';

export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const guard = await apiGuard(req, { perm: 'tile:cockpit:view::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor;
  if (!actor) return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });

  const url = new URL(req.url);
  const raw = parseInt(url.searchParams.get('days') || '90', 10);
  const days = Math.min(Math.max(isFinite(raw) ? raw : 90, 7), 365);
  const projection = await loadProjection(days);
  return NextResponse.json({ ok: true, projection, days });
}
