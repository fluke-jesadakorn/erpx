import { NextRequest, NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActor } from '@/server/guard';
import { hasPermission, loadActivePermSession, parseRoleId } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';

export async function GET(req: NextRequest) {
  const actor = await loadActor();
  const session = await loadActivePermSession(req);
  if (!actor || !session) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(session.session, PERM.user.directory.read)) {
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  }
  const { searchParams } = new URL(req.url);
  const filterDeptId = searchParams.get('dept_group_id')
    ? String(searchParams.get('dept_group_id'))
    : null;
  const filterRole = searchParams.get('role') ?? null;
  const includeInactive = searchParams.get('include_inactive') === '1';

  const params: unknown[] = [];
  const where: string[] = [];
  if (!includeInactive) where.push('u.is_active=TRUE');
  if (filterDeptId || filterRole) {
    where.push(`EXISTS (
      SELECT 1 FROM perm.user_permissions up
       WHERE up.user_id = u.id AND up.revoked_at IS NULL
         AND (up.ends_at IS NULL OR up.ends_at > now())
         ${filterDeptId ? `AND up.permission_id = $${params.length + 1}` : ''}
         ${filterRole ? `AND up.permission_id LIKE '%' || $${params.length + (filterDeptId ? 2 : 1)} || '%'` : ''}
    )`);
    if (filterDeptId) params.push(`user:dept:${filterDeptId}::allow`);
    if (filterRole) params.push(filterRole);
  }
  const whereSql = where.length ? `WHERE ${where.join(' AND ')}` : '';

  const r = await query(
    `SELECT u.id, u.employee_code, u.fullname, u.is_active,
            u.line_user_id, u.created_at, u.hired_at,
            (SELECT up.permission_id FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS dept_perm,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id
              ORDER BY (CASE WHEN ur.role_id LIKE '%::1' THEN 0
                             WHEN ur.role_id LIKE '%::2' THEN 1
                             WHEN ur.role_id LIKE '%::3' THEN 2
                             WHEN ur.role_id LIKE '%::4' THEN 3
                             WHEN ur.role_id LIKE '%::5' THEN 4
                             ELSE 5 END), ur.granted_at ASC
              LIMIT 1) AS role_id
       FROM users u
       ${whereSql}
       ORDER BY u.id`,
    params,
  );
  const users = r.rows.map((row: any) => {
    const parsed = parseRoleId(row.role_id ?? 'officer::5');
    const deptId = row.dept_perm ? row.dept_perm.replace(/^user:dept:/, '').replace(/::allow$/, '') : null;
    return {
      ...row,
      role_name: parsed?.name ?? 'officer',
      role_id: row.role_id,
      level: parsed?.level ?? 5,
      dept_id: deptId,
      dept_group_name: deptId,
      dept_group_id: deptId,
      department: deptId,
    };
  });
  return NextResponse.json({ users });
}

export async function POST(req: NextRequest) {
  const actor = await loadActor();
  const session = await loadActivePermSession(req);
  if (!actor || !session) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(session.session, PERM.user.profile.create)) {
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  }
  const body = await req.json();
  const code = (body.employee_code || '').trim();
  if (!code) return NextResponse.json({ error: 'Employee code is required' }, { status: 400 });
  if (!body.fullname?.trim()) return NextResponse.json({ error: 'Full name is required' }, { status: 400 });

  const dup = await query(`SELECT 1 FROM users WHERE employee_code=$1`, [code]);
  if (dup.rows.length > 0) return NextResponse.json({ error: 'Duplicate employee code' }, { status: 400 });

  const ins = await query(
    `INSERT INTO users (employee_code, fullname, is_active, hired_at)
     VALUES ($1, $2, TRUE, CURRENT_DATE) RETURNING id`,
    [code, body.fullname.trim()],
  );
  return NextResponse.json({ ok: true, id: ins.rows[0].id });
}
