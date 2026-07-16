import { NextRequest, NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActor } from '@/server/guard';
import { matchPerm, parseDeptFromPerms } from '@/perm/server';

export async function GET(_req: NextRequest) {
  const actor = await loadActor();
  if (!actor) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!matchPerm(actor.permissions, 'tile:departments:view::allow')) {
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  }

  const r = await query(
    `SELECT d.id, d.display_name AS name, d.display_name_th AS name_th,
            d.display_name_de AS name_de, d.monthly_budget, d.head_user_id,
            u.fullname AS head_fullname, u.employee_code AS head_code,
            (SELECT COUNT(*)::int FROM perm.user_permissions up
              WHERE up.permission_id = 'user:dept:' || d.id || '::allow'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())) AS active_members
       FROM perm.roles d
       LEFT JOIN users u ON u.id = d.head_user_id AND u.is_active
      WHERE d.id IN (
        SELECT DISTINCT split_part(id, ':', 3)
          FROM perm.permissions
         WHERE id LIKE 'user:dept:%::allow'
      )
      ORDER BY d.id ASC
      LIMIT 100`,
  );
  return NextResponse.json({ departments: r.rows });
}

export async function PATCH(req: NextRequest) {
  const actor = await loadActor();
  if (!actor) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const body = await req.json();
  if (!body.dept_group_id) {
    return NextResponse.json({ error: 'dept_group_id is required' }, { status: 400 });
  }
  if (!matchPerm(actor.permissions, 'org:dept:assign_head::allow')) {
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  }
  await query(
    `UPDATE perm.roles SET head_user_id=$1 WHERE id=$2`,
    [body.head_user_id ?? null, body.dept_group_id],
  );
  return NextResponse.json({ ok: true });
}

export { parseDeptFromPerms };
