// GET    /api/perm/users/[id]/grants  — list user's active grants + permanent perms.
// PUT    /api/perm/users/[id]/grants  — set the user's perms (mode: temporary|permanent).
//                                        body: { mode, desired_perm_ids, ends_at?, reason? }
// DELETE /api/perm/users/[id]/grants  — revoke by row id.
//                                        body: { id: number }

import { NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import {
  listActiveUserPerms,
  revokeUserPerm,
  setUserPermanentPerms,
} from '@/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function canMutate(s: ReturnType<typeof JSON.parse>['session'] | null) {
  if (!s) return false;
  return hasPermission(s, PERM.rbac.matrix.edit) || hasPermission(s, PERM.admin.system.bypass);
}

function canView(s: ReturnType<typeof JSON.parse>['session'] | null) {
  if (!s) return false;
  return hasPermission(s, PERM.rbac.matrix.view) || hasPermission(s, PERM.admin.system.bypass);
}

function defaultEndsAt(): string {
  const d = new Date();
  d.setDate(d.getDate() + 30);
  return d.toISOString();
}

export async function GET(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!canView(out.session))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id: idStr } = await ctx.params;
  const userId = parseInt(idStr, 10);
  if (!userId) return NextResponse.json({ error: 'invalid id' }, { status: 400 });

  const all = await listActiveUserPerms(userId);
  const activeIds = Array.from(new Set(
    all.filter((g) => g.revoked_at === null && (g.ends_at === null || new Date(g.ends_at).getTime() >= Date.now()))
      .map((g) => g.permission_id),
  ));

  const u = await query<{
    department: string | null; role_id: string | null;
    id: number; fullname: string; employee_code: string;
  }>(
    `SELECT u.id, u.fullname, u.employee_code,
       (SELECT ud.department_id FROM perm.user_departments ud
         WHERE ud.user_id = u.id) AS department,
       (SELECT ur.role_id FROM perm.user_roles ur
         WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
         ORDER BY ur.granted_at ASC LIMIT 1) AS role_id
       FROM users u WHERE u.id = $1`,
    [userId],
  );
  const rolesRes = await query<{ role_id: string }>(
    `SELECT role_id FROM perm.user_roles WHERE user_id = $1 ORDER BY role_id`,
    [userId],
  );
  const roleNamesRes = await query<{ role_id: string; display_name: string }>(
    `SELECT pr.id AS role_id, pr.display_name FROM perm.user_roles ur
       JOIN perm.roles pr ON pr.id = ur.role_id WHERE ur.user_id = $1
       ORDER BY pr.display_name`,
    [userId],
  );

  return NextResponse.json({
    user_id: userId,
    user: {
      id: u.rows[0]?.id ?? userId,
      fullname: u.rows[0]?.fullname ?? 'Unknown user',
      employee_code: u.rows[0]?.employee_code ?? '',
      department: u.rows[0]?.department ?? null,
      role_id: u.rows[0]?.role_id ?? null,
      perm_role_ids: rolesRes.rows.map((r) => r.role_id),
      perm_role_names: roleNamesRes.rows.map((r) => r.display_name),
    },
    grants: all,
    active_perm_ids: activeIds,
  });
}

export async function PUT(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!canMutate(out.session))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id: idStr } = await ctx.params;
  const userId = parseInt(idStr, 10);
  if (!userId) return NextResponse.json({ error: 'invalid id' }, { status: 400 });
  if (userId === out.session.user.id)
    return NextResponse.json({ error: 'Users cannot change their own access' }, { status: 403 });

  const body = await req.json().catch(() => ({}));
  const mode = body.mode === 'permanent' ? 'permanent' : 'temporary';
  const desired: string[] = Array.isArray(body.desired_perm_ids)
    ? body.desired_perm_ids.filter((s: unknown): s is string => typeof s === 'string')
    : [];
  const grantedBy = `user:${out.session.user.id}`;
  const reason = typeof body.reason === 'string' ? body.reason : undefined;
  const endsAt = typeof body.ends_at === 'string' ? body.ends_at : defaultEndsAt();

  const valid = await query<{ id: string }>(
    `SELECT id FROM perm.permissions WHERE id = ANY($1::text[])`,
    [desired],
  );
  const validIds = valid.rows.map((r) => r.id);

  if (validIds.some((p) => p.startsWith('user:dept:'))) {
    return NextResponse.json(
      { error: 'Department membership must be changed through the atomic access endpoint.' },
      { status: 400 },
    );
  }

  const result = await setUserPermanentPerms({
    user_id: userId,
    desired_perm_ids: validIds,
    ends_at: mode === 'temporary' ? endsAt : null,
    granted_by: grantedBy,
    reason,
  });

  if (result.added.length || result.removed > 0) {
    await query(
      `INSERT INTO perm.audit (kind, actor, target) VALUES ($1, $2, $3)`,
      [
        mode === 'permanent' ? 'user.perm.permanent.set' : 'user.perm.temporary.set',
        grantedBy,
        {
          user_id: userId,
          mode,
          added: result.added,
          removed: result.removed,
          ends_at: mode === 'temporary' ? endsAt : null,
          reason: reason ?? null,
        },
      ],
    );
  }

  return NextResponse.json({ ok: true, mode, ...result });
}

export async function DELETE(req: Request, ctx: { params: Promise<{ id: string }> }) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!canMutate(out.session))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const { id: idStr } = await ctx.params;
  const userId = parseInt(idStr, 10);
  if (!userId) return NextResponse.json({ error: 'invalid id' }, { status: 400 });
  if (userId === out.session.user.id)
    return NextResponse.json({ error: 'Users cannot change their own access' }, { status: 403 });

  const body = await req.json().catch(() => ({}));
  const id = Number(body.id);
  if (!id) return NextResponse.json({ error: 'invalid id' }, { status: 400 });

  const revokedBy = `user:${out.session.user.id}`;
  await revokeUserPerm(id, revokedBy);

  await query(
    `INSERT INTO perm.audit (kind, actor, target) VALUES ($1, $2, $3)`,
    [
      'user.perm.revoke',
      revokedBy,
      { user_id: userId, row_id: id },
    ],
  );

  return NextResponse.json({ ok: true });
}
