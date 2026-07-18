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

export async function GET(req: Request) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const decision = await authorize(actorOf(out), { kind: 'perm', perm: 'rbac:matrix:view::allow' });
  if (!decision.allow) return NextResponse.json({ error: decision.reason }, { status: 403 });

  const [roles, grants, counts] = await Promise.all([
    query<{
      id: string;
      display_name: string;
      description: string | null;
      kind: 'hierarchy' | 'system';
      rank: number | null;
      is_system: boolean;
      sort_order: number;
    }>(
      `SELECT id, display_name, description, kind, rank, is_system, sort_order
         FROM perm.roles ORDER BY kind, rank NULLS LAST, sort_order, id`,
    ),
    query<{ role_id: string; permission_id: string }>(
      `SELECT role_id, permission_id FROM perm.role_permissions`,
    ),
    query<{ role_id: string; count: number }>(
      `SELECT role_id, count(*)::int AS count FROM perm.user_roles GROUP BY role_id`,
    ),
  ]);

  const perms = new Map<string, { allow: string[]; deny: string[] }>();
  for (const grant of grants.rows) {
    const set = perms.get(grant.role_id) ?? { allow: [], deny: [] };
    set[effectOf(grant.permission_id) === 'deny' ? 'deny' : 'allow'].push(grant.permission_id);
    perms.set(grant.role_id, set);
  }
  const users = new Map(counts.rows.map((row) => [row.role_id, row.count]));
  return NextResponse.json({
    roles: roles.rows.map((role) => ({
      ...role,
      user_count: users.get(role.id) ?? 0,
      allow: perms.get(role.id)?.allow.sort() ?? [],
      deny: perms.get(role.id)?.deny.sort() ?? [],
    })),
  });
}

export async function POST(req: Request) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const decision = await authorize(actorOf(out), { kind: 'perm', perm: 'rbac:role:edit::allow' });
  if (!decision.allow) return NextResponse.json({ error: decision.reason }, { status: 403 });

  const body = await req.json().catch(() => ({})) as Record<string, unknown>;
  const id = String(body.id ?? body.name ?? '').trim().toLowerCase();
  const displayName = String(body.display_name ?? '').trim();
  const description = body.description == null ? null : String(body.description);
  const kind = body.kind === 'system' ? 'system' : 'hierarchy';
  const rank = kind === 'hierarchy' ? Number(body.rank ?? body.level) : null;
  const allow = Array.isArray(body.allow)
    ? body.allow.filter((value): value is string => typeof value === 'string')
    : [];
  if (!/^[a-z][a-z0-9_-]{1,40}$/.test(id) || !displayName) {
    return NextResponse.json({ error: 'A stable role id and display name are required' }, { status: 400 });
  }
  if (kind === 'hierarchy' && (!Number.isInteger(rank) || Number(rank) < 1 || Number(rank) > 7)) {
    return NextResponse.json({ error: 'Hierarchy rank must be an integer from 1 to 7' }, { status: 400 });
  }

  try {
    await withTransaction(async (q) => {
      await q(
        `INSERT INTO perm.roles
           (id, display_name, description, kind, rank, is_system, sort_order)
         VALUES ($1, $2, $3, $4, $5, false, 1000)`,
        [id, displayName, description, kind, rank],
      );
      if (allow.length) {
        await q(
          `INSERT INTO perm.role_permissions
             (role_id, role_kind, permission_id, granted_by)
           SELECT $1, $2, p.id, $3
             FROM perm.permissions p WHERE p.id = ANY($4::text[])`,
          [id, kind, `user:${out.session.user.id}`, allow],
        );
      }
      await q(
        `INSERT INTO perm.audit (kind, actor, target)
         VALUES ('role.create', $1, $2)`,
        [`user:${out.session.user.id}`, { after: { id, displayName, description, kind, rank, allow } }],
      );
    });
    return NextResponse.json({ ok: true, id }, { status: 201 });
  } catch (error) {
    const code = (error as { code?: string }).code;
    return NextResponse.json(
      { error: code === '23505' ? 'Role id or name already exists' : 'Role creation failed' },
      { status: code === '23505' ? 409 : 500 },
    );
  }
}
