// GET  /api/perm/roles      — list all roles with their granted permission ids.
// POST /api/perm/roles      — create a custom role (HR only).
// Both behind rbac:matrix:view / rbac:matrix:edit.

import { NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActivePermSession, hasPermission, PERM, effectOf } from '@/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(_req: Request) {
  const out = await loadActivePermSession(_req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, 'rbac:matrix:view::allow'))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const rolesRes = await query<{
    id: string;
    display_name: string;
    display_name_th: string | null;
    display_name_de: string | null;
    description: string | null;
    is_system: boolean;
    sort_order: number;
  }>(
    `SELECT id, display_name, display_name_th, display_name_de,
            description, is_system, sort_order
       FROM perm.roles
      ORDER BY is_system DESC, sort_order ASC, id ASC`,
  );
  const grantsRes = await query<{ role_id: string; permission_id: string }>(
    `SELECT role_id, permission_id FROM perm.role_permissions`,
  );

  const userCountRes = await query<{ role_id: string; count: number }>(
    `SELECT role_id, COUNT(*)::int AS count FROM perm.user_roles GROUP BY role_id`,
  );
  const userCountByRole: Record<string, number> = {};
  for (const r of userCountRes.rows) userCountByRole[r.role_id] = r.count;

  const grantsByRole: Record<string, { allow: string[]; deny: string[] }> = {};
  for (const g of grantsRes.rows) {
    if (!grantsByRole[g.role_id]) grantsByRole[g.role_id] = { allow: [], deny: [] };
    const eff = effectOf(g.permission_id);
    if (eff === 'deny') grantsByRole[g.role_id].deny.push(g.permission_id);
    else grantsByRole[g.role_id].allow.push(g.permission_id);
  }
  for (const k of Object.keys(grantsByRole)) {
    grantsByRole[k].allow.sort();
    grantsByRole[k].deny.sort();
  }

  return NextResponse.json({
    roles: rolesRes.rows.map((r) => {
      const levelIdx = r.id.indexOf('::');
      const level = levelIdx > 0 ? parseInt(r.id.slice(levelIdx + 2), 10) : 0;
      return {
        ...r,
        level: Number.isFinite(level) ? level : 0,
        user_count: userCountByRole[r.id] ?? 0,
        allow: grantsByRole[r.id]?.allow ?? [],
        deny: grantsByRole[r.id]?.deny ?? [],
      };
    }),
  });
}

export async function POST(req: Request) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, PERM.rbac.matrix.edit))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const body = await req.json().catch(() => ({}));
  const name = String(body.name ?? body.id ?? '').trim().toLowerCase();
  const display_name = String(body.display_name ?? '').trim();
  const description = body.description == null ? null : String(body.description);
  const level = Number.isFinite(Number(body.level)) ? Number(body.level) : 5;
  const allow: string[] = Array.isArray(body.allow)
    ? body.allow.filter((s: unknown) => typeof s === 'string')
    : [];

  if (!name) return NextResponse.json({ error: 'name is required' }, { status: 400 });
  if (!/^[a-z][a-z0-9_]{1,40}$/.test(name))
    return NextResponse.json({ error: 'name must be snake_case (a-z, 0-9, _)' }, { status: 400 });
  if (!display_name)
    return NextResponse.json({ error: 'display_name is required' }, { status: 400 });
  if (level < 1 || level > 10)
    return NextResponse.json({ error: 'level must be between 1 and 10' }, { status: 400 });

  const id = `${name}::${level}`;
  const dup = await query<{ id: string }>(`SELECT id FROM perm.roles WHERE id = $1`, [id]);
  if (dup.rows.length > 0)
    return NextResponse.json({ error: `Role "${id}" already exists` }, { status: 409 });

  const sort = 1000;
  await query(
    `INSERT INTO perm.roles (id, display_name, description, is_system, sort_order)
     VALUES ($1, $2, $3, false, $4)`,
    [id, display_name, description, sort],
  );

  if (allow.length > 0) {
    const valid = await query<{ id: string }>(
      `SELECT id FROM perm.permissions WHERE id = ANY($1::text[])`,
      [allow],
    );
    const validIds = valid.rows.map((r) => r.id);
    for (const pid of validIds) {
      await query(
        `INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
         VALUES ($1, $2, $3) ON CONFLICT DO NOTHING`,
        [id, pid, `user:${out.session.user.id}`],
      );
    }
    for (const pid of validIds) {
      await query(
        `INSERT INTO perm.audit (kind, actor, target) VALUES ('role.permission.grant', $1, $2)`,
        [`user:${out.session.user.id}`, { role_id: id, permission_id: pid }],
      );
    }
  }

  await query(
    `INSERT INTO perm.audit (kind, actor, target) VALUES ('role.create', $1, $2)`,
    [`user:${out.session.user.id}`, { role_id: id, display_name, level }],
  );

  return NextResponse.json({ ok: true, id });
}
