import { NextResponse } from 'next/server';
import { query, withTransaction } from '@/db';
import { loadActivePermSession, effectOf } from '@/perm/server';
import { authorize } from '@folio-lib/perm/authorize';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function actorOf(out: NonNullable<Awaited<ReturnType<typeof loadActivePermSession>>>) {
  return {
    id: out.session.user.id,
    permissions: out.session.permissions,
    deptId: out.session.user.department,
    level: out.session.user.rank ?? undefined,
    roleName: out.session.user.role,
  };
}

async function permit(req: Request, permission: string) {
  const out = await loadActivePermSession(req);
  if (!out) return { response: NextResponse.json({ error: 'unauthorized' }, { status: 401 }) } as const;
  const decision = await authorize(actorOf(out), { kind: 'perm', perm: permission });
  if (!decision.allow) {
    return { response: NextResponse.json({ error: decision.reason }, { status: 403 }) } as const;
  }
  return { out, response: null } as const;
}

export async function GET(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const auth = await permit(req, 'rbac:matrix:view::allow');
  if (auth.response) return auth.response;
  const { id } = await ctx.params;
  const role = await query<{
    id: string;
    display_name: string;
    description: string | null;
    kind: 'hierarchy' | 'system';
    rank: number | null;
    is_system: boolean;
    sort_order: number;
  }>(`SELECT id, display_name, description, kind, rank, is_system, sort_order FROM perm.roles WHERE id = $1`, [id]);
  if (!role.rows[0]) return NextResponse.json({ error: 'role not found' }, { status: 404 });
  const [grants, members] = await Promise.all([
    query<{ permission_id: string }>(
      `SELECT permission_id FROM perm.role_permissions WHERE role_id = $1 ORDER BY permission_id`,
      [id],
    ),
    query<{ user_id: number; fullname: string; employee_code: string }>(
      `SELECT ur.user_id, u.fullname, u.employee_code
         FROM perm.user_roles ur JOIN users u ON u.id = ur.user_id
        WHERE ur.role_id = $1 ORDER BY u.fullname`,
      [id],
    ),
  ]);
  return NextResponse.json({
    role: {
      ...role.rows[0],
      allow: grants.rows.filter((g) => effectOf(g.permission_id) !== 'deny').map((g) => g.permission_id),
      deny: grants.rows.filter((g) => effectOf(g.permission_id) === 'deny').map((g) => g.permission_id),
      members: members.rows,
    },
  });
}

export async function PATCH(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const auth = await permit(req, 'rbac:role:edit::allow');
  if (auth.response) return auth.response;
  const { id } = await ctx.params;
  const current = await query<{
    display_name: string;
    description: string | null;
    rank: number | null;
    kind: 'hierarchy' | 'system';
    is_system: boolean;
  }>(`SELECT display_name, description, rank, kind, is_system FROM perm.roles WHERE id = $1`, [id]);
  if (!current.rows[0]) return NextResponse.json({ error: 'role not found' }, { status: 404 });
  if (current.rows[0].is_system) {
    return NextResponse.json({ error: 'Canonical role definitions are protected' }, { status: 403 });
  }
  const body = await req.json().catch(() => ({})) as Record<string, unknown>;
  const displayName = body.display_name === undefined ? current.rows[0].display_name : String(body.display_name).trim();
  const description = body.description === undefined ? current.rows[0].description : body.description == null ? null : String(body.description);
  const rank = current.rows[0].kind === 'hierarchy'
    ? Number(body.rank ?? current.rows[0].rank)
    : null;
  if (!displayName || (current.rows[0].kind === 'hierarchy' && (!Number.isInteger(rank) || Number(rank) < 1 || Number(rank) > 7))) {
    return NextResponse.json({ error: 'Invalid role update' }, { status: 400 });
  }
  await withTransaction(async (q) => {
    await q(
      `UPDATE perm.roles SET display_name = $2, description = $3, rank = $4, updated_at = now() WHERE id = $1`,
      [id, displayName, description, rank],
    );
    await q(
      `INSERT INTO perm.audit (kind, actor, target) VALUES ('role.update', $1, $2)`,
      [`user:${auth.out.session.user.id}`, { id, before: current.rows[0], after: { displayName, description, rank } }],
    );
  });
  return NextResponse.json({ ok: true });
}

export async function DELETE(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const auth = await permit(req, 'rbac:role:edit::allow');
  if (auth.response) return auth.response;
  const { id } = await ctx.params;
  const role = await query<{ display_name: string; is_system: boolean }>(
    `SELECT display_name, is_system FROM perm.roles WHERE id = $1`,
    [id],
  );
  if (!role.rows[0]) return NextResponse.json({ error: 'role not found' }, { status: 404 });
  if (role.rows[0].is_system) return NextResponse.json({ error: 'Canonical roles cannot be deleted' }, { status: 403 });
  const members = await query<{ count: number }>(
    `SELECT count(*)::int AS count FROM perm.user_roles WHERE role_id = $1`,
    [id],
  );
  if ((members.rows[0]?.count ?? 0) > 0) {
    return NextResponse.json({ error: 'Reassign role members before deletion' }, { status: 409 });
  }
  await withTransaction(async (q) => {
    await q(`DELETE FROM perm.roles WHERE id = $1`, [id]);
    await q(
      `INSERT INTO perm.audit (kind, actor, target) VALUES ('role.delete', $1, $2)`,
      [`user:${auth.out.session.user.id}`, { before: { id, ...role.rows[0] } }],
    );
  });
  return NextResponse.json({ ok: true });
}
