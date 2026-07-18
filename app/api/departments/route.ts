import { NextRequest, NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActor } from '@/server/guard';
import { hasPermission, loadActivePermSession, parseDeptFromPerms } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';

export async function GET(req: NextRequest) {
  const actor = await loadActor();
  const session = await loadActivePermSession(req);
  if (!actor || !session) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(session.session, PERM.tile.departments.view)) {
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  }

  const r = await query(
    `SELECT d.id, d.display_name AS name, NULL::text AS name_th,
            NULL::text AS name_de, NULL::numeric AS monthly_budget, d.head_user_id,
            u.fullname AS head_fullname, u.employee_code AS head_code,
            (SELECT COUNT(*)::int FROM perm.user_departments ud
              JOIN users m ON m.id = ud.user_id AND m.is_active IS TRUE
             WHERE ud.department_id = d.id) AS active_members
       FROM perm.departments d
       LEFT JOIN users u ON u.id = d.head_user_id AND u.is_active
      ORDER BY d.id ASC
      LIMIT 100`,
  );
  return NextResponse.json({ departments: r.rows });
}

export async function PATCH(req: NextRequest) {
  const actor = await loadActor();
  const session = await loadActivePermSession(req);
  if (!actor || !session) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const body = await req.json();
  if (!body.dept_group_id) {
    return NextResponse.json({ error: 'dept_group_id is required' }, { status: 400 });
  }
  if (!hasPermission(session.session, PERM.org.dept.assign_head)) {
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  }
  await query(
    `UPDATE perm.departments SET head_user_id=$1, updated_at=now() WHERE id=$2`,
    [body.head_user_id ?? null, body.dept_group_id],
  );
  return NextResponse.json({ ok: true });
}

export { parseDeptFromPerms };
