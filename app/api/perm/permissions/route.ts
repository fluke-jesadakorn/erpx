// GET /api/perm/permissions — list every permission in perm.permissions,
// grouped by domain. Public to any signed-in user (used by the matrix page).

import { NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActivePermSession } from '@/perm/server';
import { parsePerm } from '@/perm/grammar';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function parsePermMeta(id: string): { domain: string; subject: string; verb: string; qualifier: string | null } {
  const p = parsePerm(id);
  if (!p) return { domain: '', subject: '', verb: '', qualifier: null };
  return { domain: p.domain, subject: p.subject, verb: p.verb, qualifier: p.qualifier };
}

export async function GET(req: Request) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });

  const res = await query<{ id: string; description: string | null }>(
    `SELECT id, description
       FROM perm.permissions
      ORDER BY id`,
  );
  return NextResponse.json({
    permissions: res.rows.map((r) => ({
      id: r.id,
      ...parsePermMeta(r.id),
      description: r.description,
    })),
  });
}
