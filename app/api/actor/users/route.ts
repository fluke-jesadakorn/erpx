// GET /api/actor/users — dev-only list of seeded users for the sign-in panel.
// Returns 404 in production.

import { NextResponse } from 'next/server';
import { query } from '@/db';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function prod(): boolean {
  return process.env.NODE_ENV === 'production';
}

export async function GET() {
  if (prod()) return NextResponse.json({ error: 'disabled' }, { status: 404 });

  const r = await query(
    `SELECT u.id, u.employee_code, u.fullname,
            d.department_id AS department,
            d.department_id AS dept_group_id,
            d.department_id AS dept_group_name,
            r.role_id, r.role_name,
            r.level AS level
       FROM users u
       LEFT JOIN LATERAL (
         SELECT ur.role_id, ur.role_id AS role_name, pr.rank AS level
           FROM perm.user_roles ur
           JOIN perm.roles pr ON pr.id = ur.role_id AND pr.kind = ur.role_kind
          WHERE ur.user_id = u.id
            AND ur.role_kind = 'hierarchy'
          LIMIT 1
       ) r ON true
       LEFT JOIN perm.user_departments d ON d.user_id = u.id
      WHERE u.is_active = TRUE
      ORDER BY r.level ASC NULLS LAST, r.role_id ASC, u.id ASC`,
  );
  return NextResponse.json({ users: r.rows, pinRequired: !!process.env.DEV_ACTOR_PIN });
}
