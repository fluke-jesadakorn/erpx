// PUT /api/perm/users/[id]/roles — replace a user's perm role set.
// Body: { roles: string[] } or { roleIds: string[] }

import { NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const ROLE_ID_REGEX = /^[a-z][a-z0-9_]{0,63}::([1-9]|10)$/;

function asRoleIds(body: unknown): string[] | null {
  const b = (body ?? {}) as { roles?: unknown; roleIds?: unknown };
  const raw = Array.isArray(b.roleIds) ? b.roleIds : Array.isArray(b.roles) ? b.roles : null;
  if (raw === null) return null;
  return raw.filter((s: unknown): s is string => typeof s === 'string');
}

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
  const roleIds = asRoleIds(body);
  if (roleIds === null) {
    return NextResponse.json(
      { error: 'invalid body', detail: 'roles[] or roleIds[] required' },
      { status: 400 },
    );
  }
  const bad = roleIds.filter(r => !ROLE_ID_REGEX.test(r));
  if (bad.length) {
    return NextResponse.json(
      { error: 'invalid_role_id', detail: 'role id must match <name>::<1-10>', invalid: bad },
      { status: 400 },
    );
  }

  if (roleIds.length === 0) {
    await query(`DELETE FROM perm.user_roles WHERE user_id = $1`, [userId]);
  } else {
    const existsRes = await query<{ id: string }>(
      `SELECT id FROM perm.roles WHERE id = ANY($1::text[])`,
      [roleIds],
    );
    const known = new Set(existsRes.rows.map(r => r.id));
    const missing = roleIds.filter(r => !known.has(r));
    if (missing.length) {
      return NextResponse.json(
        { error: 'unknown_role_id', invalid: missing },
        { status: 400 },
      );
    }

    const existing = await query<{ role_id: string }>(
      `SELECT role_id FROM perm.user_roles WHERE user_id = $1`,
      [userId],
    );
    const before = new Set(existing.rows.map((r) => r.role_id));
    const after  = new Set(roleIds);

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
