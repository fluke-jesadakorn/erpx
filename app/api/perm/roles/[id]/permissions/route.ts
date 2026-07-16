// PUT /api/perm/roles/[id]/permissions — replace the role's allow-grants.
// Body: { allow: string[] }
// Behind perm:matrix:edit. Writes an audit row per change.

import { NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActivePermSession, hasPermission, PERM, SESSION_COOKIE } from '@/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function PUT(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, PERM.rbac.matrix.edit))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id } = await ctx.params;
  const body = await req.json().catch(() => ({}));
  const allow: string[] = Array.isArray(body.allow) ? body.allow.filter((s: unknown) => typeof s === 'string') : [];

  const existing = await query<{ permission_id: string }>(
    `SELECT permission_id FROM perm.role_permissions WHERE role_id = $1`,
    [id],
  );
  const before = new Set(existing.rows.map((r) => r.permission_id));
  const after  = new Set(allow);

  const added   = [...after].filter((p) => !before.has(p));
  const removed = [...before].filter((p) => !after.has(p));

  if (removed.length) {
    await query(
      `DELETE FROM perm.role_permissions WHERE role_id = $1 AND permission_id = ANY($2::text[])`,
      [id, removed],
    );
  }
  for (const p of added) {
    await query(
      `INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
       VALUES ($1, $2, $3) ON CONFLICT DO NOTHING`,
      [id, p, `user:${out.session.user.id}`],
    );
  }

  for (const p of added) {
    await query(
      `INSERT INTO perm.audit (kind, actor, target) VALUES ('role.permission.add', $1, $2)`,
      [`user:${out.session.user.id}`, { role_id: id, permission_id: p }],
    );
  }
  for (const p of removed) {
    await query(
      `INSERT INTO perm.audit (kind, actor, target) VALUES ('role.permission.remove', $1, $2)`,
      [`user:${out.session.user.id}`, { role_id: id, permission_id: p }],
    );
  }

  void SESSION_COOKIE; // re-export touch
  return NextResponse.json({ ok: true, added, removed });
}
