// GET /api/actor/users — dev-only list of seeded users for the sign-in panel.
// Returns 404 in production.

import { NextResponse } from 'next/server';
import { query } from '@folio-lib/db';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function prod(): boolean {
  return process.env.NODE_ENV === 'production';
}

export async function GET() {
  if (prod()) return NextResponse.json({ error: 'disabled' }, { status: 404 });

  const r = await query(
    `SELECT u.id, u.employee_code, u.fullname,
            (SELECT split_part(up.permission_id, ':', 3) FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS department,
            (SELECT split_part(up.permission_id, ':', 3) FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS dept_group_id,
            (SELECT split_part(up.permission_id, ':', 3) FROM perm.user_permissions up
              WHERE up.user_id = u.id AND up.permission_id LIKE 'user:dept:%'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS dept_group_name,
            r.role_id, r.role_name,
            r.level AS level
       FROM users u
       LEFT JOIN LATERAL (
         SELECT ur.role_id, split_part(ur.role_id, '::', 1) AS role_name,
                split_part(ur.role_id, '::', 2)::int AS level
           FROM perm.user_roles ur
          WHERE ur.user_id = u.id
          ORDER BY split_part(ur.role_id, '::', 2)::int ASC
          LIMIT 1
       ) r ON true
      WHERE u.is_active = TRUE
      ORDER BY r.level ASC NULLS LAST, r.role_id ASC, u.id ASC`,
  );
  return NextResponse.json({ users: r.rows, pinRequired: !!process.env.DEV_ACTOR_PIN });
}