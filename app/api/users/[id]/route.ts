import { NextRequest, NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActor } from '@/server/guard';
import { hasPermission, loadActivePermSession, parseRoleId } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(_req: NextRequest, ctx: { params: Promise<{ id: string }> }) {
  const actor = await loadActor();
  if (!actor) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const { id } = await ctx.params;
  const r = await query(
    `SELECT u.id, u.employee_code, u.fullname, u.is_active,
            u.line_user_id, u.created_at,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id
              ORDER BY (CASE WHEN ur.role_id LIKE '%::1' THEN 0
                             WHEN ur.role_id LIKE '%::2' THEN 1
                             WHEN ur.role_id LIKE '%::3' THEN 2
                             WHEN ur.role_id LIKE '%::4' THEN 3
                             WHEN ur.role_id LIKE '%::5' THEN 4
                             ELSE 5 END), ur.granted_at ASC
              LIMIT 1) AS role_id,
            (SELECT up.permission_id FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS dept_perm
       FROM users u
       WHERE u.id=$1`,
    [id],
  );
  if (r.rows.length === 0) return NextResponse.json({ error: 'not found' }, { status: 404 });
  const row: any = r.rows[0];
  const parsed = parseRoleId(row.role_id ?? 'officer::5');
  const deptId = row.dept_perm
    ? row.dept_perm.replace(/^user:dept:/, '').replace(/::allow$/, '')
    : null;
  return NextResponse.json({
    user: {
      ...row,
      role_name: parsed?.name ?? 'officer',
      level: parsed?.level ?? 5,
      department: deptId,
      department_id: deptId,
      dept_group_id: deptId,
      dept_group_name: deptId,
    },
  });
}

export async function PATCH(req: NextRequest, ctx: { params: Promise<{ id: string }> }) {
  const actor = await loadActor();
  const session = await loadActivePermSession(req);
  if (!actor || !session) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const { id } = await ctx.params;
  const body = await req.json();

  if (!hasPermission(session.session, PERM.user.profile.update)) {
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  }

  if (body.role_id !== undefined) {
    await query(`DELETE FROM perm.user_roles WHERE user_id=$1`, [id]);
    if (body.role_id) {
      await query(`INSERT INTO perm.user_roles (user_id, role_id) VALUES ($1, $2)`, [id, body.role_id]);
    }
  }
  if (body.dept_id !== undefined && typeof body.dept_id === 'string') {
    await query(
      `UPDATE perm.user_permissions SET revoked_at=now(), revoked_by='role-update'
        WHERE user_id=$1 AND permission_id LIKE 'user:dept:%' AND revoked_at IS NULL`,
      [id],
    );
    if (body.dept_id) {
      const permId = `user:dept:${body.dept_id}::allow`;
      await query(
        `INSERT INTO perm.permissions (id) VALUES ($1) ON CONFLICT DO NOTHING`,
        [permId],
      );
      await query(
        `INSERT INTO perm.user_permissions (user_id, permission_id, granted_by, reason)
         VALUES ($1, $2, $3, 'dept binding')
         ON CONFLICT DO NOTHING`,
        [id, permId, String(actor.id)],
      );
    }
  }
  if (body.is_active !== undefined) {
    if (body.is_active === false && Number(id) === actor.id) {
      return NextResponse.json({ error: 'You cannot deactivate your own account' }, { status: 400 });
    }
    await query(`UPDATE users SET is_active=$2 WHERE id=$1`, [id, body.is_active]);
  }
  return NextResponse.json({ ok: true });
}

export async function DELETE(req: NextRequest, ctx: { params: Promise<{ id: string }> }) {
  const actor = await loadActor();
  const session = await loadActivePermSession(req);
  if (!actor || !session) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const { id } = await ctx.params;
  if (Number(id) === actor.id) {
    return NextResponse.json({ error: 'You cannot remove your own account' }, { status: 400 });
  }
  if (!hasPermission(session.session, PERM.user.profile.delete)) {
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  }

  const target = await query(`SELECT id, employee_code, fullname FROM users WHERE id=$1`, [id]);
  if (target.rows.length === 0) return NextResponse.json({ error: 'Target not found' }, { status: 404 });

  try {
    await query(`UPDATE perm.roles SET head_user_id=NULL WHERE head_user_id=$1`, [id]);
    await query(`DELETE FROM users WHERE id=$1`, [id]);
    return NextResponse.json({ ok: true });
  } catch (e: any) {
    const msg = String(e?.message || e);
    if (/foreign key|violates/i.test(msg)) {
      return NextResponse.json(
        { error: `Cannot remove ${target.rows[0].employee_code}: this employee still has expenses, approvals, or other history. Deactivate them instead.` },
        { status: 400 },
      );
    }
    return NextResponse.json({ error: msg }, { status: 500 });
  }
}
