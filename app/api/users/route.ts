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
  if (filterDeptId) {
    params.push(filterDeptId);
    where.push(`ud.department_id = $${params.length}`);
  }
  if (filterRole) {
    params.push(filterRole);
    where.push(`ur.role_id = $${params.length}`);
  }
  const whereSql = where.length ? `WHERE ${where.join(' AND ')}` : '';

  const r = await query(
    `SELECT u.id, u.employee_code, u.fullname, u.is_active,
            u.line_user_id, u.created_at, u.hired_at,
            ud.department_id,
            ur.role_id,
            pr.rank
       FROM users u
       LEFT JOIN perm.user_departments ud ON ud.user_id = u.id
       LEFT JOIN perm.user_roles ur ON ur.user_id = u.id AND ur.role_kind = 'hierarchy'
       LEFT JOIN perm.roles pr ON pr.id = ur.role_id AND pr.kind = ur.role_kind
       ${whereSql}
       ORDER BY u.id`,
    params,
  );
  const users = r.rows.map((row: any) => {
    const parsed = parseRoleId(row.role_id ?? '');
    const deptId = row.department_id ?? null;
    return {
      ...row,
      role_name: parsed?.name ?? 'unconfigured',
      role_id: row.role_id,
      level: row.rank ?? parsed?.level ?? 99,
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
