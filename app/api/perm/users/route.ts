// GET /api/perm/users — list users with role assignments.

import { NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActivePermSession, hasPermission } from '@/perm/server';

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
            (SELECT ud.department_id FROM perm.user_departments ud
              WHERE ud.user_id = u.id) AS department,
            NULL::text AS department_th,
            NULL::text AS department_de,
            COALESCE((SELECT r.rank FROM perm.user_roles ur
              JOIN perm.roles r ON r.id = ur.role_id
              WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'), 99) AS effective_level,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
              ORDER BY ur.granted_at ASC
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
  const users = res.rows.map((row) => ({
    ...row,
    department_th: row.department,
    department_de: row.department,
    dept_group_id: row.department,
  }));
  return NextResponse.json({ users });
}
