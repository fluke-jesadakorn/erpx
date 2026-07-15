// PUT /api/perm/users/[id]/roles — replace a user's perm role set.
// Body: { roles: string[] }

import { NextResponse } from 'next/server';
import { query } from '@folio-lib/db';
import { loadActivePermSession, hasPermission, PERM } from '@folio-lib/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function PUT(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (
    !hasPermission(out.session, PERM.rbac.role.assign) &&
    !hasPermission(out.session, PERM.user.role.assign)
  )
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id: idStr } = await ctx.params;
  const userId = parseInt(idStr, 10);
  if (!userId) return NextResponse.json({ error: 'invalid id' }, { status: 400 });

  const body = await req.json().catch(() => ({}));
  const roles: string[] = Array.isArray(body.roles)
    ? body.roles.filter((s: unknown) => typeof s === 'string')
    : [];

  if (roles.length === 0) {
    await query(`DELETE FROM perm.user_roles WHERE user_id = $1`, [userId]);
  } else {
    const existing = await query<{ role_id: string }>(
      `SELECT role_id FROM perm.user_roles WHERE user_id = $1`,
      [userId],
    );
    const before = new Set(existing.rows.map((r) => r.role_id));
    const after  = new Set(roles);

    const removed = [...before].filter((r) => !after.has(r));
    const added   = [...after].filter((r) => !before.has(r));

    if (removed.length) {
      await query(
        `DELETE FROM perm.user_roles WHERE user_id = $1 AND role_id = ANY($2::text[])`,
        [userId, removed],
      );
    }
    for (const r of added) {
      await query(
        `INSERT INTO perm.user_roles (user_id, role_id, granted_by) VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING`,
        [userId, r, `user:${out.session.user.id}`],
      );
    }
    if (added.length || removed.length) {
      await query(
        `INSERT INTO perm.audit (kind, actor, target) VALUES ('role.assign', $1, $2)`,
        [`user:${out.session.user.id}`, { user_id: userId, added, removed }],
      );
    }
  }

  return NextResponse.json({ ok: true });
}
