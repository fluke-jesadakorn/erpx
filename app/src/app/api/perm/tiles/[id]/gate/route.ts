// PATCH /api/perm/tiles/[id]/gate — update a tile's view_perm_id.
//
// Body: { view_perm_id: string }
// Behind perm:rbac:matrix:edit.

import { NextResponse } from 'next/server';
import { query } from '@folio-lib/db';
import { loadActivePermSession, hasPermission, PERM_ID_REGEX } from '@folio-lib/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function PATCH(
  req: Request,
  ctx: { params: Promise<{ id: string }> },
) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, 'rbac:matrix:edit::allow'))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id } = await ctx.params;
  const body = await req.json().catch(() => ({} as Record<string, unknown>));

  const viewPerm = typeof body.view_perm_id === 'string' ? body.view_perm_id : null;
  if (!viewPerm || !PERM_ID_REGEX.test(viewPerm)) {
    return NextResponse.json({ error: 'view_perm_id must be a valid perm string' }, { status: 400 });
  }

  const beforeRes = await query<{ view_perm_id: string }>(
    `SELECT view_perm_id FROM perm.tiles WHERE id = $1`,
    [id],
  );
  if (beforeRes.rows.length === 0) {
    return NextResponse.json({ error: 'tile not found' }, { status: 404 });
  }
  const before = beforeRes.rows[0].view_perm_id;

  if (before === viewPerm) {
    return NextResponse.json({ ok: true, changed: false });
  }

  await query(
    `UPDATE perm.tiles
        SET view_perm_id = $1, updated_at = now()
      WHERE id = $2`,
    [viewPerm, id],
  );

  await query(
    `INSERT INTO perm.audit (kind, actor, target) VALUES ('tile.gate.update', $1, $2)`,
    [
      `user:${out.session.user.id}`,
      { tile_id: id, before: { view_perm_id: before }, after: { view_perm_id: viewPerm } },
    ],
  );

  return NextResponse.json({ ok: true, changed: true });
}