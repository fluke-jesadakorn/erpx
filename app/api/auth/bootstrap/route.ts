import { NextResponse } from 'next/server';
import { cookies } from 'next/headers';
import { withTransaction } from '@/db';
import { mintSessionId, safeEqual, SESSION_COOKIE, SESSION_TTL_SECONDS, signSession } from '@/server/sessionToken';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const ID_RE = /^[a-z][a-z0-9_-]{1,40}$/;

interface BootstrapBody {
  employee_code?: unknown;
  fullname?: unknown;
  department_id?: unknown;
  role_id?: unknown;
}

export async function POST(req: Request) {
  const expected = process.env.FOLIO_BOOTSTRAP_TOKEN ?? '';
  const supplied = req.headers.get('x-folio-bootstrap-token') ?? '';
  if (expected.length < 32 || !safeEqual(supplied, expected)) {
    return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });
  }

  const body = await req.json().catch(() => ({})) as BootstrapBody;
  const employeeCode = String(body.employee_code ?? '').trim();
  const fullname = String(body.fullname ?? '').trim();
  const departmentId = String(body.department_id ?? '').trim().toLowerCase();
  const roleId = String(body.role_id ?? '').trim().toLowerCase();
  if (
    employeeCode.length < 1 || employeeCode.length > 80 ||
    fullname.length < 1 || fullname.length > 200 ||
    !ID_RE.test(departmentId) || !ID_RE.test(roleId)
  ) {
    return NextResponse.json({ ok: false, error: 'invalid bootstrap input' }, { status: 400 });
  }

  try {
    const created = await withTransaction(async (q) => {
      await q('LOCK TABLE users IN SHARE ROW EXCLUSIVE MODE');
      const active = await q<{ count: number }>(
        `SELECT count(*)::int AS count FROM users WHERE is_active IS TRUE`,
      );
      if ((active.rows[0]?.count ?? 0) > 0) throw new Error('bootstrap_closed');

      const role = await q<{ id: string; department_id: string }>(
        `SELECT r.id, r.department_id
           FROM perm.roles r
          WHERE r.id = $1
            AND r.kind = 'hierarchy'
            AND r.department_id = $2
            AND EXISTS (
              SELECT 1 FROM perm.role_permissions rp
               WHERE rp.role_id = r.id
                 AND rp.role_kind = r.kind
                 AND rp.permission_id = 'user:profile:create::allow'
            )
            AND EXISTS (
              SELECT 1 FROM perm.role_permissions rp
               WHERE rp.role_id = r.id
                 AND rp.role_kind = r.kind
                 AND rp.permission_id = 'rbac:role:assign::allow'
            )`,
        [roleId, departmentId],
      );
      if (!role.rows[0]) throw new Error('bootstrap_role_not_allowed');

      const dept = await q<{ id: string }>(
        `SELECT id FROM perm.departments WHERE id = $1`,
        [departmentId],
      );
      if (!dept.rows[0]) throw new Error('bootstrap_department_not_found');

      const user = await q<{ id: number }>(
        `INSERT INTO users (employee_code, fullname, is_active, hired_at)
         VALUES ($1, $2, TRUE, CURRENT_DATE)
         RETURNING id`,
        [employeeCode, fullname],
      );
      const userId = user.rows[0].id;
      await q(
        `INSERT INTO perm.user_departments (user_id, department_id, assigned_by)
         VALUES ($1, $2, NULL)`,
        [userId, departmentId],
      );
      await q(
        `INSERT INTO perm.user_roles (user_id, role_id, role_kind, granted_by)
         VALUES ($1, $2, 'hierarchy', 'bootstrap')`,
        [userId, roleId],
      );
      await q(
        `INSERT INTO perm.user_permissions (user_id, permission_id, granted_by, reason)
         VALUES ($1, $2, 'bootstrap', 'Initial access department compatibility grant')`,
        [userId, `user:dept:${departmentId}::allow`],
      );

      const sessionId = mintSessionId();
      await q(
        `INSERT INTO auth.sessions (id, user_id, role, expires_at)
         VALUES ($1, $2, $3, now() + ($4 || ' seconds')::interval)`,
        [sessionId, userId, roleId, SESSION_TTL_SECONDS],
      );
      await q(
        `INSERT INTO perm.audit (kind, actor, target)
         VALUES ('auth.bootstrap', 'bootstrap', $1)`,
        [{ userId, departmentId, roleId }],
      );
      return { userId, sessionId, roleId };
    });

    const token = await signSession({
      id: created.sessionId,
      sub: created.userId,
      role: created.roleId,
      impersonatorUserId: null,
    });
    const jar = await cookies();
    jar.set(SESSION_COOKIE, token, {
      httpOnly: true,
      sameSite: 'lax',
      secure: process.env.NODE_ENV === 'production',
      maxAge: SESSION_TTL_SECONDS,
      path: '/',
    });
    return NextResponse.json({ ok: true, userId: created.userId, roleId: created.roleId });
  } catch (error) {
    const code = error instanceof Error ? error.message : 'bootstrap_failed';
    if (code === 'bootstrap_closed') {
      return NextResponse.json({ ok: false, error: 'bootstrap is already closed' }, { status: 409 });
    }
    if (code === 'bootstrap_role_not_allowed' || code === 'bootstrap_department_not_found') {
      return NextResponse.json({ ok: false, error: 'bootstrap role or department is not allowed' }, { status: 400 });
    }
    if (/duplicate key|unique constraint/i.test(code)) {
      return NextResponse.json({ ok: false, error: 'employee code already exists' }, { status: 409 });
    }
    console.error('bootstrap failed:', error);
    return NextResponse.json({ ok: false, error: 'bootstrap failed' }, { status: 500 });
  }
}
