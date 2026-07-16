import { NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActor } from '@/server/guard';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET() {
  const actor = await loadActor();
  if (!actor) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const r = await query(
    `SELECT id, display_name, subtitle, icon, accent, group_name, sub_view,
            href, NULL::text AS module_id, request_target, sort_order, is_system,
            owner_group_id, view_perm_id
       FROM perm.tiles
       ORDER BY sort_order ASC, id ASC`,
  );
  return NextResponse.json({
    tiles: r.rows.map((t) => ({
      ...t,
      access_meta: { viewPermId: t.view_perm_id },
    })),
  });
}