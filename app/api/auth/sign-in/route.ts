// POST /api/auth/sign-in   { id, pin? }
// Dev-only sign-in: sets the folio_session cookie carrying the new
// { user, permissions } payload. In production this is disabled — real
// auth would set the cookie server-side after credential verification.

import { NextResponse } from 'next/server';
import { revalidatePath } from 'next/cache';
import { cookies } from 'next/headers';
import { query } from '@/db';
import {
  signSession, SESSION_COOKIE, SESSION_TTL_SECONDS, safeEqual, mintSessionId, verifySession,
} from '@/server/sessionToken';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function prod(): boolean {
  return process.env.NODE_ENV === 'production';
}

export async function POST(req: Request) {
  if (prod()) {
    return NextResponse.json({ error: 'disabled in production' }, { status: 404 });
  }

  const body = await req.json().catch(() => ({}));
  const id = parseInt(body.id, 10);
  if (!id) return NextResponse.json({ error: 'id required' }, { status: 400 });

  const expectedPin = process.env.DEV_ACTOR_PIN;
  if (expectedPin) {
    const provided = String(body.pin || '');
    if (!safeEqual(provided, expectedPin)) {
      return NextResponse.json({ error: 'invalid pin' }, { status: 401 });
    }
  }

  const r = await query<{ id: number; fullname: string; role_id: string | null }>(
    `SELECT u.id, u.fullname,
            (SELECT ur.role_id FROM perm.user_roles ur
              WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
              LIMIT 1) AS role_id
       FROM users u
      WHERE u.id = $1 AND u.is_active IS TRUE`,
    [id],
  );
  if (!r.rows.length) return NextResponse.json({ error: 'user not found' }, { status: 404 });

  const row = r.rows[0];
  const roleId = row.role_id ?? 'unconfigured';
  const { rows: permRows } = await query<{ permission_id: string }>(
    `SELECT DISTINCT p_id AS permission_id FROM (
        SELECT rp.permission_id AS p_id
          FROM perm.user_roles ur
          JOIN perm.role_permissions rp ON rp.role_id = ur.role_id
         WHERE ur.user_id = $1
        UNION
        SELECT permission_id AS p_id
          FROM perm.user_permissions
         WHERE user_id = $1 AND revoked_at IS NULL
           AND (ends_at IS NULL OR ends_at > now())
        UNION
        SELECT dp.permission_id AS p_id
          FROM perm.user_departments ud
          JOIN perm.department_permissions dp ON dp.department_id = ud.department_id
         WHERE ud.user_id = $1
        UNION
        SELECT 'user:dept:' || ud.department_id || '::allow' AS p_id
          FROM perm.user_departments ud
         WHERE ud.user_id = $1
     ) t
     ORDER BY p_id`,
    [row.id],
  );
  const permissions = permRows.map((p) => p.permission_id);

  const c = await cookies();
  const previous = await verifySession(c.get(SESSION_COOKIE)?.value ?? null);
  if (previous) {
    await query(`UPDATE auth.sessions SET revoked_at = now() WHERE id = $1`, [previous.id]).catch(() => {});
  }

  const sid = mintSessionId();
  await query(
    `INSERT INTO auth.sessions (id, user_id, role, expires_at)
     VALUES ($1, $2, $3, now() + ($4 || ' seconds')::interval)`,
    [sid, row.id, roleId, SESSION_TTL_SECONDS],
  );

  const token = await signSession({
    id: sid,
    sub: row.id,
    role: roleId,
    impersonatorUserId: null,
  });

  c.set(SESSION_COOKIE, token, {
    httpOnly: true,
    sameSite: 'lax',
    secure: process.env.NODE_ENV === 'production',
    maxAge: SESSION_TTL_SECONDS,
    path: '/',
  });

  revalidatePath('/', 'layout');
  return NextResponse.json({
    ok: true,
    user: { id: row.id, fullname: row.fullname, role: roleId, permissions },
  });
}
