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
              WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
              LIMIT 1) AS role_id,
            (SELECT pr.rank FROM perm.user_roles ur
              JOIN perm.roles pr ON pr.id = ur.role_id AND pr.kind = ur.role_kind
             WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
             LIMIT 1) AS rank,
            (SELECT ud.department_id FROM perm.user_departments ud
              WHERE ud.user_id = u.id) AS department_id
       FROM users u
       WHERE u.id=$1`,
    [id],
  );
  if (r.rows.length === 0) return NextResponse.json({ error: 'not found' }, { status: 404 });
  const row: any = r.rows[0];
  const parsed = parseRoleId(row.role_id ?? '');
  const deptId = row.department_id ?? null;
  return NextResponse.json({
    user: {
      ...row,
      role_name: parsed?.name ?? 'unconfigured',
      level: row.rank ?? parsed?.level ?? 99,
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

  if (body.role_id !== undefined || body.dept_id !== undefined) {
    return NextResponse.json(
      { error: 'Use PUT /api/perm/users/[id]/access for access assignments' },
      { status: 400 },
    );
  }
  if (body.is_active !== undefined) {
    if (body.is_active === false && Number(id) === actor.id) {
      return NextResponse.json({ error: 'You cannot deactivate your own account' }, { status: 400 });
    }
    if (body.is_active === false) {
      const admin = await query<{ is_access_admin: boolean; other_count: number }>(
        `SELECT EXISTS (
           SELECT 1 FROM perm.user_departments
            WHERE user_id = $1 AND department_id IN ('hr', 'it')
         ) AS is_access_admin,
         (SELECT count(*)::int
            FROM perm.user_departments ud
            JOIN users u ON u.id = ud.user_id AND u.is_active IS TRUE
            JOIN perm.user_roles ur ON ur.user_id = ud.user_id AND ur.role_kind = 'hierarchy'
           WHERE ud.department_id IN ('hr', 'it') AND ud.user_id <> $1) AS other_count`,
        [id],
      );
      if (admin.rows[0]?.is_access_admin && admin.rows[0].other_count === 0) {
        return NextResponse.json({ error: 'The final access administrator cannot be deactivated' }, { status: 409 });
      }
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
    await query(`UPDATE perm.departments SET head_user_id=NULL WHERE head_user_id=$1`, [id]);
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
