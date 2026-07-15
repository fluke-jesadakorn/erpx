// GET /api/perm/audit — recent perm.audit rows.

import { NextResponse } from 'next/server';
import { query } from '@folio-lib/db';
import { loadActivePermSession, hasPermission } from '@folio-lib/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, 'rbac:audit:view::allow'))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const limit = Math.min(parseInt(new URL(req.url).searchParams.get('limit') || '50', 10), 200);
  const res = await query<{ id: number; kind: string; actor: string; target: unknown; occurred_at: string }>(
    `SELECT id, kind, actor, target, occurred_at FROM perm.audit
      ORDER BY occurred_at DESC LIMIT $1`,
    [limit],
  );
  return NextResponse.json({ events: res.rows });
}
