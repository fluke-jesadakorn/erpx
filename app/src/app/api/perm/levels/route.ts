// POST /api/perm/levels — batch-fetch effective level for many users.
// Body: { user_ids: number[] }
// Response: { levels: Record<userId, level> }

import { NextResponse } from 'next/server';
import { loadActivePermSession, hasPermission, PERM } from '@folio-lib/perm/server';
import { getEffectiveLevels } from '@folio-lib/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: Request) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, PERM.rbac.matrix.view))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const body = await req.json().catch(() => ({}));
  const ids: number[] = Array.isArray(body.user_ids)
    ? body.user_ids.filter((n: unknown) => Number.isFinite(Number(n))).map((n: unknown) => Number(n))
    : [];

  if (ids.length > 1000)
    return NextResponse.json({ error: 'too many user_ids (max 1000)' }, { status: 400 });

  const map = await getEffectiveLevels(ids);
  const levels: Record<number, number> = {};
  for (const [k, v] of map.entries()) levels[k] = v;
  return NextResponse.json({ levels });
}