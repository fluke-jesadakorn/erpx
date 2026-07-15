// GET /api/perm/permissions — list every permission in perm.permissions,
// grouped by domain. Public to any signed-in user (used by the matrix page).

import { NextResponse } from 'next/server';
import { query } from '@folio-lib/db';
import { loadActivePermSession } from '@folio-lib/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });

  const res = await query<{ id: string; domain: string; subject: string; verb: string; description: string | null }>(
    `SELECT id, domain, subject, verb, description
       FROM perm.permissions
      ORDER BY domain, subject, verb, id`,
  );
  return NextResponse.json({ permissions: res.rows });
}
