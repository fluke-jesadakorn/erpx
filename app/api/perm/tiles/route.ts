// GET /api/perm/tiles — full tile catalog with current view_perm_id.

import { NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActivePermSession, hasPermission } from '@/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface TileRow {
  id: string;
  display_name: string;
  subtitle: string;
  icon: string;
  accent: string;
  group_name: string;
  href: string;
  view_perm_id: string;
  request_target: string | null;
  sort_order: number;
}

export async function GET(req: Request) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, 'rbac:matrix:view::allow'))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const [tilesRes, deptsRes] = await Promise.all([
    query<TileRow>(
      `SELECT t.id, t.display_name, COALESCE(t.subtitle,'') AS subtitle,
              COALESCE(t.icon,'🧾') AS icon, COALESCE(t.accent,'slate') AS accent,
              t.group_name, COALESCE(t.href,'') AS href,
              t.view_perm_id,
              COALESCE(t.request_target,'') AS request_target,
              t.sort_order
         FROM perm.tiles t
        ORDER BY t.sort_order, t.id`,
    ),
    query<{ id: string; display_name: string }>(
      `SELECT DISTINCT split_part(id, ':', 3) AS id,
              split_part(id, ':', 3) AS display_name
         FROM perm.permissions
        WHERE id LIKE 'user:dept:%::allow'
        ORDER BY id`,
    ),
  ]);

  return NextResponse.json({
    tiles: tilesRes.rows,
    departments: deptsRes.rows,
  });
}