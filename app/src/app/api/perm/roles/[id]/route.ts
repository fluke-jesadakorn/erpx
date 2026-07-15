// GET    /api/perm/roles/[id]            — single role detail (full perm list + members).
// PATCH  /api/perm/roles/[id]            — rename / description / level (changes role-id).
// DELETE /api/perm/roles/[id]            — delete custom role (cascades).

import { NextResponse } from 'next/server';
import { query } from '@folio-lib/db';
import {
  loadActivePermSession, hasPermission, PERM, effectOf, parseRoleId,
} from '@folio-lib/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, PERM.rbac.matrix.view))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id } = await ctx.params;

  const roleRes = await query<{
    id: string;
    display_name: string;
    description: string | null;
    is_system: boolean;
    sort_order: number;
  }>(
    `SELECT id, display_name, description, is_system, sort_order
       FROM perm.roles WHERE id = $1`,
    [id],
  );
  if (roleRes.rows.length === 0)
    return NextResponse.json({ error: 'role not found' }, { status: 404 });

  const grantsRes = await query<{ permission_id: string }>(
    `SELECT permission_id FROM perm.role_permissions WHERE role_id = $1`,
    [id],
  );
  const allow = grantsRes.rows.filter((g) => effectOf(g.permission_id) !== 'deny')
    .map((g) => g.permission_id).sort();
  const deny = grantsRes.rows.filter((g) => effectOf(g.permission_id) === 'deny')
    .map((g) => g.permission_id).sort();

  const membersRes = await query<{ user_id: number; fullname: string; employee_code: string }>(
    `SELECT ur.user_id, u.fullname, u.employee_code
       FROM perm.user_roles ur
       JOIN users u ON u.id = ur.user_id
      WHERE ur.role_id = $1
      ORDER BY u.fullname`,
    [id],
  );

  const parsed = parseRoleId(id);
  return NextResponse.json({
    role: {
      ...roleRes.rows[0],
      level: parsed?.level ?? 0,
      allow,
      deny,
      members: membersRes.rows,
    },
  });
}

export async function PATCH(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, PERM.rbac.matrix.edit))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id } = await ctx.params;
  const body = await req.json().catch(() => ({}));

  const cur = await query<{ is_system: boolean; display_name: string }>(
    `SELECT is_system, display_name FROM perm.roles WHERE id = $1`,
    [id],
  );
  if (cur.rows.length === 0)
    return NextResponse.json({ error: 'role not found' }, { status: 404 });
  const row = cur.rows[0];

  const updates: string[] = [];
  const params: unknown[] = [];
  const audit: Record<string, unknown> = { role_id: id };

  if (body.display_name !== undefined) {
    if (row.is_system)
      return NextResponse.json({ error: 'System role display_name cannot be changed' }, { status: 403 });
    const dn = String(body.display_name).trim();
    if (!dn) return NextResponse.json({ error: 'display_name cannot be empty' }, { status: 400 });
    params.push(dn);
    updates.push(`display_name = $${params.length}`);
    audit.display_name = dn;
  }
  if (body.description !== undefined) {
    params.push(body.description == null ? null : String(body.description));
    updates.push(`description = $${params.length}`);
    audit.description = body.description;
  }

  // Level change = role-id rename (since level is encoded in id).
  let renamedTo: string | null = null;
  if (body.level !== undefined) {
    const lv = Number(body.level);
    if (!Number.isFinite(lv) || lv < 1 || lv > 10)
      return NextResponse.json({ error: 'level must be between 1 and 10' }, { status: 400 });
    const parsed = parseRoleId(id);
    if (!parsed)
      return NextResponse.json({ error: 'role-id is not in <name>::<level> format' }, { status: 400 });
    renamedTo = `${parsed.name}::${lv}`;
    audit.level = lv;
    audit.renamed_to = renamedTo;
    if (row.is_system) audit.warning = 'system_role_level_changed';
  }

  if (updates.length === 0 && !renamedTo)
    return NextResponse.json({ ok: true, changed: 0 });

  if (updates.length > 0) {
    params.push(id);
    await query(
      `UPDATE perm.roles SET ${updates.join(', ')} WHERE id = $${params.length}`,
      params,
    );
  }

  if (renamedTo && renamedTo !== id) {
    await query(`UPDATE perm.roles SET id = $1 WHERE id = $2`, [renamedTo, id]);
  }

  await query(
    `INSERT INTO perm.audit (kind, actor, target) VALUES ('role.update', $1, $2)`,
    [`user:${out.session.user.id}`, audit],
  );

  return NextResponse.json({ ok: true, renamed_to: renamedTo });
}

export async function DELETE(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, PERM.rbac.matrix.edit))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id } = await ctx.params;

  const cur = await query<{ is_system: boolean; display_name: string }>(
    `SELECT is_system, display_name FROM perm.roles WHERE id = $1`,
    [id],
  );
  if (cur.rows.length === 0)
    return NextResponse.json({ error: 'role not found' }, { status: 404 });
  if (cur.rows[0].is_system)
    return NextResponse.json(
      { error: `System role "${cur.rows[0].display_name}" cannot be deleted` },
      { status: 403 },
    );

  const memberCount = await query<{ count: number }>(
    `SELECT COUNT(*)::int AS count FROM perm.user_roles WHERE role_id = $1`,
    [id],
  );

  await query(`DELETE FROM perm.roles WHERE id = $1`, [id]);

  await query(
    `INSERT INTO perm.audit (kind, actor, target) VALUES ('role.delete', $1, $2)`,
    [
      `user:${out.session.user.id}`,
      { role_id: id, display_name: cur.rows[0].display_name, cascaded_members: memberCount.rows[0].count },
    ],
  );

  return NextResponse.json({ ok: true, cascaded_members: memberCount.rows[0].count });
}
