// GET /api/perm/users — list users with role assignments.

import { NextResponse } from 'next/server';
import { query } from '@folio-lib/db';
import { loadActivePermSession, hasPermission } from '@folio-lib/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const out = await loadActivePermSession(req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, 'rbac:matrix:view::allow'))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const res = await query<{
    id: number;
    fullname: string;
    employee_code: string;
    is_active: boolean;
    department: string | null;
    department_th: string | null;
    department_de: string | null;
    effective_level: number;
    role_id: string | null;
    perm_role_ids: string[];
    perm_role_names: string[];
  }>(
    `SELECT u.id, u.fullname, u.employee_code, u.is_active,
            (SELECT up.permission_id FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS dept_perm,
            NULL::text AS department_th,
            NULL::text AS department_de,
            COALESCE((
              SELECT MIN(split_part(ur.role_id, '::', 2)::int)
                FROM perm.user_roles ur WHERE ur.user_id = u.id
            ), 5) AS effective_level,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id
              ORDER BY (CASE WHEN ur.role_id LIKE '%::1' THEN 0
                             WHEN ur.role_id LIKE '%::2' THEN 1
                             WHEN ur.role_id LIKE '%::3' THEN 2
                             WHEN ur.role_id LIKE '%::4' THEN 3
                             WHEN ur.role_id LIKE '%::5' THEN 4
                             ELSE 5 END), ur.granted_at ASC
              LIMIT 1) AS role_id,
            COALESCE((SELECT array_agg(ur.role_id ORDER BY ur.role_id)
                        FROM perm.user_roles ur WHERE ur.user_id = u.id),
                      ARRAY[]::text[]) AS perm_role_ids,
            COALESCE((SELECT array_agg(pr.display_name ORDER BY pr.display_name)
                        FROM perm.user_roles ur JOIN perm.roles pr ON pr.id = ur.role_id
                       WHERE ur.user_id = u.id),
                      ARRAY[]::text[]) AS perm_role_names
       FROM users u
       ORDER BY u.id`,
  );
  const users = res.rows.map((row: any) => ({
    ...row,
    department: row.dept_perm
      ? row.dept_perm.replace(/^user:dept:/, '').replace(/::allow$/, '')
      : null,
    department_th: row.dept_perm
      ? row.dept_perm.replace(/^user:dept:/, '').replace(/::allow$/, '')
      : null,
    department_de: row.dept_perm
      ? row.dept_perm.replace(/^user:dept:/, '').replace(/::allow$/, '')
      : null,
    dept_group_id: row.dept_perm
      ? row.dept_perm.replace(/^user:dept:/, '').replace(/::allow$/, '')
      : null,
  }));
  return NextResponse.json({ users });
}